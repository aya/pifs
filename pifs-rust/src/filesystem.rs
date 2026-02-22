use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEmpty,
    ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, ReplyXattr, Request, TimeOrNow,
};

use crate::ipfs;
use crate::types::{PifsFile, PifsFilesystem};

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
}

/// FOPEN_DIRECT_IO flag — bypass kernel page cache.
const FOPEN_DIRECT_IO: u32 = 1;

impl Filesystem for PifsFilesystem {
    fn init(
        &mut self,
        _req: &Request,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        log::info!("pifs filesystem initialized");
        Ok(())
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
                    if let Some(pf) = self.files.get(&ino) {
                        attr.size = pf.size as u64;
                    } else if let Some(&cached) = self.size_cache.get(&ino) {
                        attr.size = cached as u64;
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
                        self.register_inode(attr.ino, mdf);
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
                    self.register_inode(attr.ino, mdf);
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
                    self.register_inode(attr.ino, dst);
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
                    self.register_inode(meta.ino(), new_path);
                }
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
            if let Some(pf) = self.files.get_mut(&ino) {
                pf.data.resize(new_size as usize, 0);
                pf.size = new_size;
                pf.dirty = true;
                pf.write_frontier = new_size as usize;
            } else {
                let mut data = Vec::new();
                if let Ok(hashes) = read_hashes(&mdf) {
                    if !hashes.is_empty() {
                        if let Ok(d) = ipfs::ipfs_cat(&hashes) {
                            data = d;
                        }
                    }
                }
                data.resize(new_size as usize, 0);
                let frontier = new_size as usize;
                self.files.insert(
                    ino,
                    PifsFile {
                        data,
                        size: new_size,
                        dirty: true,
                        write_frontier: frontier,
                    },
                );
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

        // Return updated attrs
        match std::fs::symlink_metadata(&mdf) {
            Ok(meta) => {
                let mut attr = metadata_to_attr(&meta);
                if let Some(pf) = self.files.get(&ino) {
                    attr.size = pf.size as u64;
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
                self.register_inode(ino, mdf);

                self.files.insert(ino, PifsFile::new());
                self.size_cache.insert(ino, 0);

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

        // Load file data from IPFS
        let access_mode = flags & libc::O_ACCMODE;
        if access_mode == libc::O_RDONLY {
            match read_hashes(&mdf) {
                Ok(hashes) if !hashes.is_empty() => match ipfs::ipfs_cat(&hashes) {
                    Ok(data) => {
                        let pf = PifsFile::with_data(data);
                        self.size_cache.insert(ino, pf.size);
                        self.files.insert(ino, pf);
                    }
                    Err(e) => {
                        unsafe { libc::close(fd) };
                        reply.error(e);
                        return;
                    }
                },
                Ok(_) => {
                    self.files.insert(ino, PifsFile::new());
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
                let has_data = self.size_cache.get(&ino).map_or(false, |&s| s > 0);
                if has_data {
                    match read_hashes(&mdf) {
                        Ok(hashes) if !hashes.is_empty() => {
                            if let Ok(data) = ipfs::ipfs_cat(&hashes) {
                                self.files.insert(ino, PifsFile::with_data(data));
                            } else {
                                self.files.insert(ino, PifsFile::new());
                            }
                        }
                        _ => {
                            self.files.insert(ino, PifsFile::new());
                        }
                    }
                } else {
                    self.files.insert(ino, PifsFile::new());
                }
            }
        }

        log::info!("open ino={} fd={} flags={:o}", ino, fd, flags);
        reply.opened(fd as u64, FOPEN_DIRECT_IO);
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
        let pf = match self.files.get(&ino) {
            Some(f) => f,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let offset = offset as usize;
        if offset >= pf.data.len() {
            reply.data(&[]);
            return;
        }

        let end = std::cmp::min(offset + size as usize, pf.data.len());
        let data = &pf.data[offset..end];
        log::info!("read ino={} offset={} size={} actual={}", ino, offset, size, data.len());
        reply.data(data);
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
        let pf = match self.files.get_mut(&ino) {
            Some(f) => f,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let offset = offset as usize;
        let end = offset + data.len();

        // Detect kernel page cache replays: if this write is below the
        // write frontier and the buffer already has data there, the kernel
        // is replaying stale cached pages. Skip these writes to preserve
        // the correct data that was written in the first pass.
        if offset < pf.write_frontier && end <= pf.data.len() {
            log::debug!(
                "write ino={} offset={} count={} SKIPPED (replay below frontier={})",
                ino, offset, data.len(), pf.write_frontier
            );
            reply.written(data.len() as u32);
            return;
        }

        let needed = end;
        if needed > pf.data.len() {
            pf.data.resize(needed, 0);
        }
        pf.data[offset..end].copy_from_slice(data);
        if end > pf.write_frontier {
            pf.write_frontier = end;
        }
        pf.size = pf.data.len() as i64;
        pf.dirty = true;

        log::info!("write ino={} offset={} count={} size={}", ino, offset, data.len(), pf.size);
        reply.written(data.len() as u32);
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

        if let Some(pf) = self.files.get(&ino) {
            if pf.dirty {
                if let Some(ref mdf_path) = mdf {
                    match ipfs::ipfs_add(&pf.data) {
                        Ok(hash) => {
                            log::info!("release ino={} hash={}", ino, hash);
                            if let Err(e) = std::fs::write(mdf_path, format!("{}\n", hash)) {
                                log::error!("failed to write hash to {:?}: {}", mdf_path, e);
                            }
                            self.size_cache.insert(ino, pf.size);
                        }
                        Err(e) => {
                            log::error!("ipfs add failed for ino={}: {}", ino, e);
                        }
                    }
                }
            }
        }

        self.files.remove(&ino);
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
            if let Ok(meta) = entry.metadata() {
                let ft = if meta.is_dir() {
                    FileType::Directory
                } else if meta.file_type().is_symlink() {
                    FileType::Symlink
                } else {
                    FileType::RegularFile
                };
                all_entries.push((meta.ino(), ft, entry.file_name().to_string_lossy().to_string()));
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
        let c_path = std::ffi::CString::new(mdf.as_os_str().as_bytes()).unwrap();
        let c_name = std::ffi::CString::new(name.as_bytes()).unwrap();

        if size == 0 {
            let ret = unsafe {
                libc::getxattr(c_path.as_ptr(), c_name.as_ptr(), std::ptr::null_mut(), 0, 0, 0)
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
                    0,
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
        let c_path = std::ffi::CString::new(mdf.as_os_str().as_bytes()).unwrap();
        let c_name = std::ffi::CString::new(name.as_bytes()).unwrap();

        if size == 0 {
            let ret = unsafe {
                libc::getxattr(c_path.as_ptr(), c_name.as_ptr(), std::ptr::null_mut(), 0)
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
        // causing EINVAL. Strip invalid bits.
        let safe_flags = sanitize_xattr_flags(flags);

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

        let ret = unsafe {
            libc::setxattr(
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

        if size == 0 {
            #[cfg(target_os = "macos")]
            let ret = unsafe { libc::listxattr(c_path.as_ptr(), std::ptr::null_mut(), 0, 0) };
            #[cfg(target_os = "linux")]
            let ret = unsafe { libc::listxattr(c_path.as_ptr(), std::ptr::null_mut(), 0) };

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
                    0,
                )
            };
            #[cfg(target_os = "linux")]
            let ret = unsafe {
                libc::listxattr(
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

        #[cfg(target_os = "macos")]
        let ret = unsafe { libc::removexattr(c_path.as_ptr(), c_name.as_ptr(), 0) };
        #[cfg(target_os = "linux")]
        let ret = unsafe { libc::removexattr(c_path.as_ptr(), c_name.as_ptr()) };

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
}
