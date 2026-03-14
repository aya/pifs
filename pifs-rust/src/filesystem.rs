use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEmpty,
    ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, ReplyXattr, Request, TimeOrNow,
};

use crate::file_data::{self, StorageMode};
use crate::git_sync::GitEvent;
use crate::ipfs;
use crate::types::PifsFilesystem;

const TTL: Duration = Duration::from_secs(0);

/// Convert std::fs::Metadata to fuser::FileAttr.
fn metadata_to_attr(meta: &std::fs::Metadata) -> FileAttr {
    let kind = if meta.is_dir() {
        FileType::Directory
    } else if meta.is_symlink() {
        FileType::Symlink
    } else {
        FileType::RegularFile
    };

    FileAttr {
        ino: meta.ino(),
        size: meta.size(),
        blocks: meta.blocks(),
        atime: UNIX_EPOCH + Duration::from_secs(meta.atime() as u64),
        mtime: UNIX_EPOCH + Duration::from_secs(meta.mtime() as u64),
        ctime: UNIX_EPOCH + Duration::from_secs(meta.ctime() as u64),
        crtime: UNIX_EPOCH,
        kind,
        perm: meta.mode() as u16,
        nlink: meta.nlink() as u32,
        uid: meta.uid(),
        gid: meta.gid(),
        rdev: meta.rdev() as u32,
        blksize: meta.blksize() as u32,
        flags: 0,
    }
}

/// Read hashes from a metadata file (one 46-char hash per line).
fn read_hashes(path: &Path) -> Result<Vec<String>, i32> {
    let content = std::fs::read_to_string(path).map_err(|_| libc::EIO)?;
    let mut hashes = Vec::new();
    for line in content.lines() {
        let h = line.trim();
        if !h.is_empty() {
            if h.len() != 46 {
                log::error!("hash length ({}) not supported: {}", h.len(), h);
                return Err(libc::ENOENT);
            }
            hashes.push(h.to_string());
        }
    }
    Ok(hashes)
}

// ─── Inode → Path helpers ────────────────────────────────────

impl PifsFilesystem {
    fn resolve_ino(&self, ino: u64) -> Option<PathBuf> {
        if ino == 1 {
            Some(self.mdd.clone())
        } else {
            self.inode_paths.get(&ino).cloned()
        }
    }

    pub fn register_inode(&mut self, ino: u64, path: PathBuf) {
        self.inode_paths.insert(ino, path);
    }

    /// Try to get the `ipfs.hash` xattr for a file, computing lazily if needed.
    /// Returns Some(hash_bytes) if computed, None to fall through to passthrough.
    fn get_ipfs_hash(&self, ino: u64, mdf: &Path) -> Option<Vec<u8>> {
        if !mdf.is_file() {
            return None;
        }
        // If xattr already exists on the MDD file, let passthrough handle it
        if file_data::get_xattr(mdf, "ipfs.hash").is_some() {
            return None;
        }
        // Compute lazily: read hashes from MDD, get size, compute whole-file hash
        let hashes = read_hashes(mdf).ok()?;
        if hashes.is_empty() {
            return None;
        }
        let total_size = self.size_cache.get(&ino).copied()
            .or_else(|| file_data::get_ipfs_size_xattr(mdf))
            .unwrap_or_else(|| {
                hashes.iter()
                    .filter_map(|h| ipfs::ipfs_file_size(h).ok())
                    .sum()
            });
        match file_data::compute_ipfs_hash(mdf, &hashes, total_size) {
            Ok(hash) => Some(hash.into_bytes()),
            Err(_) => None,
        }
    }

    /// Try to get the `ipfs.size` xattr for a file, computing if needed.
    /// Returns Some(size_bytes) if available, None to fall through to passthrough.
    fn get_ipfs_size(&self, ino: u64, mdf: &Path) -> Option<Vec<u8>> {
        if !mdf.is_file() {
            return None;
        }
        // If xattr already exists on the MDD file, let passthrough handle it
        if file_data::get_xattr(mdf, "ipfs.size").is_some() {
            return None;
        }
        // Compute: from in-memory cache, or ipfs files stat
        let size = self.size_cache.get(&ino).copied()
            .or_else(|| {
                let hashes = read_hashes(mdf).ok()?;
                if hashes.is_empty() {
                    return None;
                }
                let total: i64 = hashes.iter()
                    .filter_map(|h| ipfs::ipfs_file_size(h).ok())
                    .sum();
                file_data::set_ipfs_size_xattr(mdf, total);
                Some(total)
            })?;
        Some(size.to_string().into_bytes())
    }
}

/// FOPEN_DIRECT_IO flag — bypass kernel page cache.
const FOPEN_DIRECT_IO: u32 = 1;

/// FOPEN_PURGE_UBC — macOS-specific flag to purge the unified buffer cache on open.
/// This ensures stale cached pages are invalidated so the kernel re-reads from FUSE.
#[cfg(target_os = "macos")]
const FOPEN_PURGE_UBC: u32 = 1 << 31;

/// Choose FOPEN flags for an open() call based on the access mode in `flags`.
///
/// Read-only opens avoid DIRECT_IO so that mmap/execve works (DIRECT_IO prevents
/// demand paging, causing SIGBUS on binary execution). On macOS we set PURGE_UBC
/// to invalidate stale page cache entries.
///
/// Write-mode opens keep DIRECT_IO to prevent kernel page cache replay corruption.
///
/// Both modes use the same strategy: DIRECT_IO for writes (prevents page cache
/// replay corruption), no DIRECT_IO for reads (allows mmap/execve — DIRECT_IO
/// prevents demand paging, causing SIGBUS). On macOS, PURGE_UBC invalidates
/// stale cached pages on read-only opens.
fn open_flags_for(flags: i32, _mode: StorageMode) -> u32 {
    let accmode = flags & libc::O_ACCMODE;
    if accmode == libc::O_RDONLY {
        #[cfg(target_os = "macos")]
        { FOPEN_PURGE_UBC }
        #[cfg(not(target_os = "macos"))]
        { 0 }
    } else {
        FOPEN_DIRECT_IO
    }
}

impl Filesystem for PifsFilesystem {
    fn init(
        &mut self,
        _req: &Request,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        log::info!("pifs filesystem initialized");
        Ok(())
    }

    fn destroy(&mut self) {
        log::info!("pifs filesystem destroying");
        if let Some(ref mut gs) = self.git_sync {
            gs.shutdown();
        }
    }

    // ─── Metadata ────────────────────────────────────────────

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        match std::fs::symlink_metadata(&mdf) {
            Ok(meta) => {
                let mut attr = metadata_to_attr(&meta);

                // For regular files, override size with IPFS size
                if meta.is_file() && meta.size() > 0 {
                    if let Some(fd) = self.files.get(&ino) {
                        attr.size = fd.size() as u64;
                    } else if let Some(&cached) = self.size_cache.get(&ino) {
                        attr.size = cached as u64;
                    } else if let Some(size) = file_data::get_ipfs_size_xattr(&mdf) {
                        self.size_cache.insert(ino, size);
                        attr.size = size as u64;
                    } else {
                        match read_hashes(&mdf) {
                            Ok(hashes) => {
                                let mut total: i64 = 0;
                                for h in &hashes {
                                    match ipfs::ipfs_file_size(h) {
                                        Ok(s) => total += s,
                                        Err(_) => {
                                            reply.error(libc::EIO);
                                            return;
                                        }
                                    }
                                }
                                self.size_cache.insert(ino, total);
                                // Persist for future mounts
                                file_data::set_ipfs_size_xattr(&mdf, total);
                                attr.size = total as u64;
                            }
                            Err(e) => {
                                reply.error(e);
                                return;
                            }
                        }
                    }
                }

                log::info!("getattr mdf={:?} ino={} size={}", mdf, ino, attr.size);
                reply.attr(&TTL, &attr);
            }
            Err(e) => {
                log::error!("getattr lstat failed: {:?}: {}", mdf, e);
                reply.error(e.raw_os_error().unwrap_or(libc::EIO));
            }
        }
    }

    fn lookup(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        reply: ReplyEntry,
    ) {

        let parent_path = match self.resolve_ino(parent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let child_path = parent_path.join(name);
        match std::fs::symlink_metadata(&child_path) {
            Ok(meta) => {
                let ino = meta.ino();
                let mut attr = metadata_to_attr(&meta);

                self.register_inode(ino, child_path.clone());

                // For regular files, override size with IPFS size
                if meta.is_file() && meta.size() > 0 {
                    if let Some(&cached) = self.size_cache.get(&ino) {
                        attr.size = cached as u64;
                    } else if let Some(size) = file_data::get_ipfs_size_xattr(&child_path) {
                        self.size_cache.insert(ino, size);
                        attr.size = size as u64;
                    } else if let Ok(hashes) = read_hashes(&child_path) {
                        let mut total: i64 = 0;
                        let mut ok = true;
                        for h in &hashes {
                            match ipfs::ipfs_file_size(h) {
                                Ok(s) => total += s,
                                Err(_) => {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        if ok {
                            self.size_cache.insert(ino, total);
                            file_data::set_ipfs_size_xattr(&child_path, total);
                            attr.size = total as u64;
                        }
                    }
                }

                log::info!("lookup parent={} name={:?} ino={}", parent, name, ino);
                reply.entry(&TTL, &attr, 0);
            }
            Err(e) => {
                reply.error(e.raw_os_error().unwrap_or(libc::ENOENT));
            }
        }
    }

    fn readlink(&mut self, _req: &Request, ino: u64, reply: ReplyData) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        match std::fs::read_link(&mdf) {
            Ok(target) => {
                log::info!("readlink mdf={:?} target={:?}", mdf, target);
                reply.data(target.as_os_str().as_bytes());
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn access(&mut self, _req: &Request, ino: u64, mask: i32, reply: ReplyEmpty) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let c_path = match std::ffi::CString::new(mdf.as_os_str().as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::EINVAL);
                return;
            }
        };
        let ret = unsafe { libc::access(c_path.as_ptr(), mask) };
        if ret == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
        } else {
            log::info!("access mdf={:?} mask={}", mdf, mask);
            reply.ok();
        }
    }

    fn statfs(&mut self, _req: &Request, ino: u64, reply: ReplyStatfs) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let c_path = match std::ffi::CString::new(mdf.as_os_str().as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::EINVAL);
                return;
            }
        };
        let mut vfs: libc::statvfs = unsafe { std::mem::zeroed() };
        let ret = unsafe { libc::statvfs(c_path.as_ptr(), &mut vfs) };
        if ret == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
        } else {
            reply.statfs(
                vfs.f_blocks as u64,
                vfs.f_bfree as u64,
                vfs.f_bavail as u64,
                vfs.f_files as u64,
                vfs.f_ffree as u64,
                vfs.f_bsize as u32,
                vfs.f_namemax as u32,
                vfs.f_frsize as u32,
            );
        }
    }

    // ─── File creation / deletion ────────────────────────────

    fn mknod(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        rdev: u32,
        reply: ReplyEntry,
    ) {
        let parent_path = match self.resolve_ino(parent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let mdf = parent_path.join(name);
        let c_path = match std::ffi::CString::new(mdf.as_os_str().as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::EINVAL);
                return;
            }
        };
        let ret = unsafe { libc::mknod(c_path.as_ptr(), mode as libc::mode_t, rdev as libc::dev_t) };
        if ret == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
            return;
        }

        match std::fs::symlink_metadata(&mdf) {
            Ok(meta) => {
                let attr = metadata_to_attr(&meta);
                self.register_inode(attr.ino, mdf);
                reply.entry(&TTL, &attr, 0);
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn mkdir(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let parent_path = match self.resolve_ino(parent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let mdf = parent_path.join(name);

        match std::fs::create_dir(&mdf) {
            Ok(()) => {
                if let Ok(c_path) = std::ffi::CString::new(mdf.as_os_str().as_bytes()) {
                    unsafe {
                        libc::chmod(c_path.as_ptr(), (mode | libc::S_IFDIR as u32) as libc::mode_t);
                    };
                }

                match std::fs::symlink_metadata(&mdf) {
                    Ok(meta) => {
                        let attr = metadata_to_attr(&meta);
                        self.register_inode(attr.ino, mdf.clone());
                        self.git_notify(GitEvent::DirCreated(mdf));
                        reply.entry(&TTL, &attr, 0);
                    }
                    Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
                }
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let parent_path = match self.resolve_ino(parent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let mdf = parent_path.join(name);

        match std::fs::remove_file(&mdf) {
            Ok(()) => {
                log::info!("unlink mdf={:?}", mdf);
                self.git_notify(GitEvent::FileDeleted(mdf));
                reply.ok();
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn rmdir(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let parent_path = match self.resolve_ino(parent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let mdf = parent_path.join(name);

        match std::fs::remove_dir(&mdf) {
            Ok(()) => {
                log::info!("rmdir mdf={:?}", mdf);
                self.git_notify(GitEvent::DirDeleted(mdf));
                reply.ok();
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn symlink(
        &mut self,
        _req: &Request,
        parent: u64,
        link_name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
        let parent_path = match self.resolve_ino(parent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let mdf = parent_path.join(link_name);

        match std::os::unix::fs::symlink(target, &mdf) {
            Ok(()) => match std::fs::symlink_metadata(&mdf) {
                Ok(meta) => {
                    let attr = metadata_to_attr(&meta);
                    self.register_inode(attr.ino, mdf.clone());
                    self.git_notify(GitEvent::FileChanged(mdf));
                    reply.entry(&TTL, &attr, 0);
                }
                Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
            },
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn link(
        &mut self,
        _req: &Request,
        ino: u64,
        newparent: u64,
        newname: &OsStr,
        reply: ReplyEntry,
    ) {
        let src = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let parent_path = match self.resolve_ino(newparent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let dst = parent_path.join(newname);

        match std::fs::hard_link(&src, &dst) {
            Ok(()) => match std::fs::symlink_metadata(&dst) {
                Ok(meta) => {
                    let attr = metadata_to_attr(&meta);
                    self.register_inode(attr.ino, dst.clone());
                    self.git_notify(GitEvent::FileChanged(dst));
                    reply.entry(&TTL, &attr, 0);
                }
                Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
            },
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn rename(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        flags: u32,
        reply: ReplyEmpty,
    ) {
        let parent_path = match self.resolve_ino(parent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let newparent_path = match self.resolve_ino(newparent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let old = parent_path.join(name);
        let new_path = newparent_path.join(newname);

        // RENAME_NOREPLACE: fail if target exists
        #[cfg(target_os = "linux")]
        const RENAME_NOREPLACE: u32 = 1;
        #[cfg(not(target_os = "linux"))]
        const RENAME_NOREPLACE: u32 = 0; // not used on macOS
        if flags & RENAME_NOREPLACE != 0 && new_path.exists() {
            reply.error(libc::EEXIST);
            return;
        }

        log::info!("rename old={:?} new={:?} flags={}", old, new_path, flags);
        match std::fs::rename(&old, &new_path) {
            Ok(()) => {
                if let Ok(meta) = std::fs::symlink_metadata(&new_path) {
                    self.register_inode(meta.ino(), new_path.clone());
                }
                self.git_notify(GitEvent::Renamed {
                    from: old,
                    to: new_path,
                });
                reply.ok();
            }
            Err(e) => {
                log::error!("rename failed: {:?} -> {:?}: {}", old, new_path, e);
                reply.error(e.raw_os_error().unwrap_or(libc::EIO));
            }
        }
    }

    // ─── Permissions / times ─────────────────────────────────

    fn setattr(
        &mut self,
        _req: &Request,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let c_path = match std::ffi::CString::new(mdf.as_os_str().as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        // chmod
        if let Some(m) = mode {
            let ret = unsafe { libc::chmod(c_path.as_ptr(), m as libc::mode_t) };
            if ret == -1 {
                reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
                return;
            }
        }

        // chown
        if uid.is_some() || gid.is_some() {
            let u = uid.unwrap_or(u32::MAX);
            let g = gid.unwrap_or(u32::MAX);
            let ret = unsafe { libc::chown(c_path.as_ptr(), u, g) };
            if ret == -1 {
                reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
                return;
            }
        }

        // truncate
        if let Some(new_size) = size {
            let new_size = new_size as i64;
            if let Some(fd) = self.files.get_mut(&ino) {
                fd.truncate(new_size as usize);
            } else {
                // File not open — need to load, truncate, and store
                let hashes = read_hashes(&mdf).unwrap_or_default();
                match file_data::open_file(self.storage_mode, hashes, None) {
                    Ok(mut fd) => {
                        fd.truncate(new_size as usize);
                        self.files.insert(ino, fd);
                    }
                    Err(e) => {
                        reply.error(e);
                        return;
                    }
                }
            }
            self.size_cache.insert(ino, new_size);
        }

        // utimens
        if atime.is_some() || mtime.is_some() {
            let to_timespec = |t: Option<TimeOrNow>| -> libc::timespec {
                match t {
                    Some(TimeOrNow::SpecificTime(st)) => {
                        let d = st.duration_since(UNIX_EPOCH).unwrap_or_default();
                        libc::timespec {
                            tv_sec: d.as_secs() as libc::time_t,
                            tv_nsec: d.subsec_nanos() as libc::c_long,
                        }
                    }
                    Some(TimeOrNow::Now) => libc::timespec {
                        tv_sec: 0,
                        tv_nsec: libc::UTIME_NOW,
                    },
                    None => libc::timespec {
                        tv_sec: 0,
                        tv_nsec: libc::UTIME_OMIT,
                    },
                }
            };
            let times = [to_timespec(atime), to_timespec(mtime)];
            let ret = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), 0) };
            if ret == -1 {
                reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
                return;
            }
        }

        // Notify git for metadata changes (chmod/chown/utimes), NOT truncate
        if mode.is_some() || uid.is_some() || gid.is_some() || atime.is_some() || mtime.is_some() {
            self.git_notify(GitEvent::MetadataChanged(mdf.clone()));
        }

        // Return updated attrs
        match std::fs::symlink_metadata(&mdf) {
            Ok(meta) => {
                let mut attr = metadata_to_attr(&meta);
                if let Some(fd) = self.files.get(&ino) {
                    attr.size = fd.size() as u64;
                } else if let Some(&cached) = self.size_cache.get(&ino) {
                    attr.size = cached as u64;
                }
                reply.attr(&TTL, &attr);
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    // ─── File I/O ────────────────────────────────────────────

    fn create(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        let parent_path = match self.resolve_ino(parent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let mdf = parent_path.join(name);

        let c_path = match std::ffi::CString::new(mdf.as_os_str().as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::EINVAL);
                return;
            }
        };
        let fd = unsafe { libc::creat(c_path.as_ptr(), mode as libc::mode_t) };
        if fd == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
            return;
        }

        match std::fs::symlink_metadata(&mdf) {
            Ok(meta) => {
                let ino = meta.ino();
                let attr = metadata_to_attr(&meta);
                self.register_inode(ino, mdf.clone());

                self.files.insert(ino, file_data::new_file(self.storage_mode));
                self.size_cache.insert(ino, 0);

                self.git_notify(GitEvent::FileChanged(mdf));

                log::info!("create ino={} fd={} mode={:o}", ino, fd, mode);
                reply.created(&TTL, &attr, 0, fd as u64, FOPEN_DIRECT_IO);
            }
            Err(e) => {
                unsafe { libc::close(fd) };
                reply.error(e.raw_os_error().unwrap_or(libc::EIO));
            }
        }
    }

    fn open(&mut self, _req: &Request, ino: u64, flags: i32, reply: ReplyOpen) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let c_path = match std::ffi::CString::new(mdf.as_os_str().as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::EINVAL);
                return;
            }
        };
        let fd = unsafe { libc::open(c_path.as_ptr(), flags) };
        if fd == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
            return;
        }

        // Load file data from IPFS via FileData backend
        let access_mode = flags & libc::O_ACCMODE;
        if access_mode == libc::O_RDONLY {
            match read_hashes(&mdf) {
                Ok(hashes) => {
                    let size_hint = self.size_cache.get(&ino).copied();
                    match file_data::open_file(self.storage_mode, hashes, size_hint) {
                        Ok(file) => {
                            self.size_cache.insert(ino, file.size());
                            self.files.insert(ino, file);
                        }
                        Err(e) => {
                            unsafe { libc::close(fd) };
                            reply.error(e);
                            return;
                        }
                    }
                }
                Err(e) => {
                    unsafe { libc::close(fd) };
                    reply.error(e);
                    return;
                }
            }
        } else {
            // For write mode, load existing data if available
            if !self.files.contains_key(&ino) {
                let hashes = read_hashes(&mdf).unwrap_or_default();
                let size_hint = self.size_cache.get(&ino).copied();
                match file_data::open_file(self.storage_mode, hashes, size_hint) {
                    Ok(file) => {
                        self.files.insert(ino, file);
                    }
                    Err(_) => {
                        self.files.insert(ino, file_data::new_file(self.storage_mode));
                    }
                }
            }
        }

        let open_flags = open_flags_for(flags, self.storage_mode);
        log::info!("open ino={} fd={} flags={:o} open_flags={:#x}", ino, fd, flags, open_flags);
        reply.opened(fd as u64, open_flags);
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let fd = match self.files.get_mut(&ino) {
            Some(f) => f,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let offset = offset as usize;
        match fd.read(offset, size as usize) {
            Ok(data) => {
                log::info!("read ino={} offset={} size={} actual={}", ino, offset, size, data.len());
                reply.data(&data);
            }
            Err(e) => reply.error(e),
        }
    }

    fn write(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        let fd = match self.files.get_mut(&ino) {
            Some(f) => f,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let offset = offset as usize;
        match fd.write(offset, data) {
            Ok(written) => {
                log::info!("write ino={} offset={} count={} size={}", ino, offset, data.len(), fd.size());
                reply.written(written as u32);
            }
            Err(e) => reply.error(e),
        }
    }

    fn flush(&mut self, _req: &Request, _ino: u64, _fh: u64, _lock_owner: u64, reply: ReplyEmpty) {
        reply.ok();
    }

    fn release(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        let mdf = self.resolve_ino(ino);

        if let Some(mut fd) = self.files.remove(&ino) {
            if fd.is_dirty() {
                if let Some(ref mdf_path) = mdf {
                    match fd.flush_to_ipfs(mdf_path) {
                        Ok(size) => {
                            log::info!("release ino={} flushed size={}", ino, size);
                            self.size_cache.insert(ino, size);
                            self.git_notify(GitEvent::FileChanged(mdf_path.clone()));
                        }
                        Err(e) => {
                            log::error!("flush_to_ipfs failed for ino={}: {}", ino, e);
                        }
                    }
                }
            }
        }
        unsafe { libc::close(fh as i32) };

        log::info!("release ino={}", ino);
        reply.ok();
    }

    fn fsync(&mut self, _req: &Request, ino: u64, fh: u64, datasync: bool, reply: ReplyEmpty) {
        let ret = if datasync {
            // fdatasync not available on macOS, use fsync
            #[cfg(target_os = "linux")]
            { unsafe { libc::fdatasync(fh as i32) } }
            #[cfg(not(target_os = "linux"))]
            { unsafe { libc::fsync(fh as i32) } }
        } else {
            unsafe { libc::fsync(fh as i32) }
        };

        if ret == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
        } else {
            log::info!("fsync ino={} datasync={}", ino, datasync);
            reply.ok();
        }
    }

    // ─── Directories ─────────────────────────────────────────

    fn opendir(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        match std::fs::read_dir(&mdf) {
            Ok(_) => {
                log::info!("opendir ino={} mdf={:?}", ino, mdf);
                reply.opened(0, 0);
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let entries = match std::fs::read_dir(&mdf) {
            Ok(rd) => rd,
            Err(e) => {
                reply.error(e.raw_os_error().unwrap_or(libc::EIO));
                return;
            }
        };

        let mut all_entries: Vec<(u64, FileType, String)> = Vec::new();
        all_entries.push((ino, FileType::Directory, ".".to_string()));
        all_entries.push((ino, FileType::Directory, "..".to_string()));

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Ok(meta) = entry.metadata() {
                let ft = if meta.is_dir() {
                    FileType::Directory
                } else if meta.file_type().is_symlink() {
                    FileType::Symlink
                } else {
                    FileType::RegularFile
                };
                all_entries.push((meta.ino(), ft, name));
            }
        }

        for (i, (entry_ino, ft, name)) in all_entries.iter().enumerate().skip(offset as usize) {
            if reply.add(*entry_ino, (i + 1) as i64, *ft, name) {
                break;
            }
        }

        reply.ok();
    }

    fn releasedir(&mut self, _req: &Request, ino: u64, _fh: u64, _flags: i32, reply: ReplyEmpty) {
        log::info!("releasedir ino={}", ino);
        reply.ok();
    }

    fn fsyncdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        datasync: bool,
        reply: ReplyEmpty,
    ) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let c_path = match std::ffi::CString::new(mdf.as_os_str().as_bytes()) {
            Ok(p) => p,
            Err(_) => {
                reply.error(libc::EINVAL);
                return;
            }
        };
        let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY) };
        if fd == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
            return;
        }

        let ret = if datasync {
            #[cfg(target_os = "linux")]
            { unsafe { libc::fdatasync(fd) } }
            #[cfg(not(target_os = "linux"))]
            { unsafe { libc::fsync(fd) } }
        } else {
            unsafe { libc::fsync(fd) }
        };
        unsafe { libc::close(fd) };

        if ret == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
        } else {
            reply.ok();
        }
    }

    // ─── xattrs ──────────────────────────────────────────────

    #[cfg(target_os = "macos")]
    fn getxattr(
        &mut self,
        _req: &Request,
        ino: u64,
        name: &OsStr,
        size: u32,
        reply: ReplyXattr,
    ) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Lazy computation of ipfs.hash / ipfs.size
        let virtual_xattr = if name == "ipfs.hash" {
            self.get_ipfs_hash(ino, &mdf)
        } else if name == "ipfs.size" {
            self.get_ipfs_size(ino, &mdf)
        } else {
            None
        };
        if let Some(value) = virtual_xattr {
            if size == 0 {
                reply.size(value.len() as u32);
            } else {
                reply.data(&value);
            }
            return;
        }

        let c_path = std::ffi::CString::new(mdf.as_os_str().as_bytes()).unwrap();
        let c_name = std::ffi::CString::new(name.as_bytes()).unwrap();

        // Use XATTR_NOFOLLOW to avoid following symlinks in the MDD.
        // Without this, getxattr on a MDD symlink whose target is on the
        // FUSE mount would re-enter the mount and deadlock (single-threaded).
        let opts = libc::XATTR_NOFOLLOW;

        if size == 0 {
            let ret = unsafe {
                libc::getxattr(c_path.as_ptr(), c_name.as_ptr(), std::ptr::null_mut(), 0, 0, opts)
            };
            if ret == -1 {
                reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
            } else {
                reply.size(ret as u32);
            }
        } else {
            let mut buf = vec![0u8; size as usize];
            let ret = unsafe {
                libc::getxattr(
                    c_path.as_ptr(),
                    c_name.as_ptr(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    size as usize,
                    0,
                    opts,
                )
            };
            if ret == -1 {
                reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
            } else {
                reply.data(&buf[..ret as usize]);
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn getxattr(
        &mut self,
        _req: &Request,
        ino: u64,
        name: &OsStr,
        size: u32,
        reply: ReplyXattr,
    ) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Lazy computation of ipfs.hash / ipfs.size
        let virtual_xattr = if name == "ipfs.hash" {
            self.get_ipfs_hash(ino, &mdf)
        } else if name == "ipfs.size" {
            self.get_ipfs_size(ino, &mdf)
        } else {
            None
        };
        if let Some(value) = virtual_xattr {
            if size == 0 {
                reply.size(value.len() as u32);
            } else {
                reply.data(&value);
            }
            return;
        }

        let c_path = std::ffi::CString::new(mdf.as_os_str().as_bytes()).unwrap();
        let c_name = std::ffi::CString::new(name.as_bytes()).unwrap();

        // Use lgetxattr to avoid following symlinks in the MDD.
        if size == 0 {
            let ret = unsafe {
                libc::lgetxattr(c_path.as_ptr(), c_name.as_ptr(), std::ptr::null_mut(), 0)
            };
            if ret == -1 {
                reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
            } else {
                reply.size(ret as u32);
            }
        } else {
            let mut buf = vec![0u8; size as usize];
            let ret = unsafe {
                libc::lgetxattr(
                    c_path.as_ptr(),
                    c_name.as_ptr(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    size as usize,
                )
            };
            if ret == -1 {
                reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
            } else {
                reply.data(&buf[..ret as usize]);
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn setxattr(
        &mut self,
        _req: &Request,
        ino: u64,
        name: &OsStr,
        value: &[u8],
        flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let c_path = std::ffi::CString::new(mdf.as_os_str().as_bytes()).unwrap();
        let c_name = std::ffi::CString::new(name.as_bytes()).unwrap();

        // FUSE may pass flags not valid for macOS setxattr (e.g. 0x8),
        // causing EINVAL. Strip invalid bits, then add XATTR_NOFOLLOW
        // to avoid following symlinks in the MDD (prevents deadlock).
        let safe_flags = sanitize_xattr_flags(flags) | libc::XATTR_NOFOLLOW;

        let ret = unsafe {
            libc::setxattr(
                c_path.as_ptr(),
                c_name.as_ptr(),
                value.as_ptr() as *const libc::c_void,
                value.len(),
                0,
                safe_flags,
            )
        };
        if ret == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
        } else {
            reply.ok();
        }
    }

    #[cfg(target_os = "linux")]
    fn setxattr(
        &mut self,
        _req: &Request,
        ino: u64,
        name: &OsStr,
        value: &[u8],
        flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let c_path = std::ffi::CString::new(mdf.as_os_str().as_bytes()).unwrap();
        let c_name = std::ffi::CString::new(name.as_bytes()).unwrap();

        // Use lsetxattr to avoid following symlinks in the MDD.
        let ret = unsafe {
            libc::lsetxattr(
                c_path.as_ptr(),
                c_name.as_ptr(),
                value.as_ptr() as *const libc::c_void,
                value.len(),
                flags,
            )
        };
        if ret == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
        } else {
            reply.ok();
        }
    }

    fn listxattr(&mut self, _req: &Request, ino: u64, size: u32, reply: ReplyXattr) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let c_path = std::ffi::CString::new(mdf.as_os_str().as_bytes()).unwrap();

        // Use NOFOLLOW/l-variants to avoid following symlinks in the MDD.
        if size == 0 {
            #[cfg(target_os = "macos")]
            let ret = unsafe { libc::listxattr(c_path.as_ptr(), std::ptr::null_mut(), 0, libc::XATTR_NOFOLLOW) };
            #[cfg(target_os = "linux")]
            let ret = unsafe { libc::llistxattr(c_path.as_ptr(), std::ptr::null_mut(), 0) };

            if ret == -1 {
                reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
            } else {
                reply.size(ret as u32);
            }
        } else {
            let mut buf = vec![0u8; size as usize];

            #[cfg(target_os = "macos")]
            let ret = unsafe {
                libc::listxattr(
                    c_path.as_ptr(),
                    buf.as_mut_ptr() as *mut libc::c_char,
                    size as usize,
                    libc::XATTR_NOFOLLOW,
                )
            };
            #[cfg(target_os = "linux")]
            let ret = unsafe {
                libc::llistxattr(
                    c_path.as_ptr(),
                    buf.as_mut_ptr() as *mut libc::c_char,
                    size as usize,
                )
            };

            if ret == -1 {
                reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
            } else {
                reply.data(&buf[..ret as usize]);
            }
        }
    }

    fn removexattr(&mut self, _req: &Request, ino: u64, name: &OsStr, reply: ReplyEmpty) {
        let mdf = match self.resolve_ino(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let c_path = std::ffi::CString::new(mdf.as_os_str().as_bytes()).unwrap();
        let c_name = std::ffi::CString::new(name.as_bytes()).unwrap();

        // Use NOFOLLOW/l-variants to avoid following symlinks in the MDD.
        #[cfg(target_os = "macos")]
        let ret = unsafe { libc::removexattr(c_path.as_ptr(), c_name.as_ptr(), libc::XATTR_NOFOLLOW) };
        #[cfg(target_os = "linux")]
        let ret = unsafe { libc::lremovexattr(c_path.as_ptr(), c_name.as_ptr()) };

        if ret == -1 {
            reply.error(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO));
        } else {
            reply.ok();
        }
    }
}

/// Filter FUSE xattr flags to only keep flags valid for the platform's setxattr.
/// FUSE may pass flags (e.g. 0x8 XATTR_NOSECURITY) that are invalid on macOS,
/// causing EINVAL.
#[cfg(target_os = "macos")]
fn sanitize_xattr_flags(flags: i32) -> i32 {
    // macOS setxattr only supports: XATTR_NOFOLLOW(0x1), XATTR_CREATE(0x2), XATTR_REPLACE(0x4)
    flags & 0x07
}

#[cfg(target_os = "linux")]
fn sanitize_xattr_flags(flags: i32) -> i32 {
    // Linux setxattr supports: XATTR_CREATE(0x1), XATTR_REPLACE(0x2)
    flags & 0x03
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_xattr_flags_strips_invalid_fuse_flags() {
        // FUSE sends 0x8 for com.apple.quarantine — invalid for macOS setxattr
        assert_eq!(sanitize_xattr_flags(0x8), 0);
    }

    #[test]
    fn test_sanitize_xattr_flags_preserves_valid_flags() {
        // XATTR_CREATE and XATTR_REPLACE are valid on both platforms
        assert_eq!(sanitize_xattr_flags(0x1), 0x1);
        assert_eq!(sanitize_xattr_flags(0x2), 0x2);
        // Both
        assert_eq!(sanitize_xattr_flags(0x3), 0x3);
        // XATTR_NOFOLLOW (0x4) is macOS-only
        #[cfg(target_os = "macos")]
        assert_eq!(sanitize_xattr_flags(0x4), 0x4);
        #[cfg(target_os = "linux")]
        assert_eq!(sanitize_xattr_flags(0x4), 0x0);
    }

    #[test]
    fn test_sanitize_xattr_flags_mixed_valid_and_invalid() {
        // 0x1 | 0x8 = 0x9 — should keep only 0x1
        assert_eq!(sanitize_xattr_flags(0x9), 0x1);
    }

    #[test]
    fn test_sanitize_xattr_flags_zero_unchanged() {
        assert_eq!(sanitize_xattr_flags(0), 0);
    }

    #[test]
    fn test_open_flags_rdonly_no_direct_io_whole_file() {
        // Read-only opens in WholeFile mode must NOT set DIRECT_IO
        let flags = open_flags_for(libc::O_RDONLY, StorageMode::WholeFile);
        assert_eq!(flags & FOPEN_DIRECT_IO, 0, "O_RDONLY WholeFile must not set DIRECT_IO");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_open_flags_rdonly_purges_ubc_on_macos() {
        let flags = open_flags_for(libc::O_RDONLY, StorageMode::WholeFile);
        assert_ne!(flags & FOPEN_PURGE_UBC, 0, "O_RDONLY WholeFile on macOS must set PURGE_UBC");
    }

    #[test]
    fn test_open_flags_wronly_uses_direct_io() {
        let flags = open_flags_for(libc::O_WRONLY, StorageMode::WholeFile);
        assert_ne!(flags & FOPEN_DIRECT_IO, 0, "O_WRONLY must set DIRECT_IO");
    }

    #[test]
    fn test_open_flags_rdwr_uses_direct_io() {
        let flags = open_flags_for(libc::O_RDWR, StorageMode::WholeFile);
        assert_ne!(flags & FOPEN_DIRECT_IO, 0, "O_RDWR must set DIRECT_IO");
    }

    #[test]
    fn test_open_flags_chunked_same_as_whole_file() {
        // Chunked mode uses the same flags as whole-file: no DIRECT_IO for reads
        // (mmap/execve needs demand paging), DIRECT_IO for writes
        let rdonly = open_flags_for(libc::O_RDONLY, StorageMode::Chunked);
        assert_eq!(rdonly & FOPEN_DIRECT_IO, 0, "Chunked O_RDONLY must not set DIRECT_IO");
        let wronly = open_flags_for(libc::O_WRONLY, StorageMode::Chunked);
        assert_ne!(wronly & FOPEN_DIRECT_IO, 0, "Chunked O_WRONLY must set DIRECT_IO");
        let rdwr = open_flags_for(libc::O_RDWR, StorageMode::Chunked);
        assert_ne!(rdwr & FOPEN_DIRECT_IO, 0, "Chunked O_RDWR must set DIRECT_IO");
    }
}
