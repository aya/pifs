use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::ipfs;

pub const CHUNK_SIZE: usize = 256 * 1024; // 256 KB
const XATTR_IPFS_HASH: &str = "ipfs.hash";
const XATTR_IPFS_SIZE: &str = "ipfs.size";

/// Storage mode: whole-file (legacy) or chunked (default).
#[derive(Debug, Clone, Copy)]
pub enum StorageMode {
    WholeFile,
    Chunked,
}

/// Trait for file data backends.
///
/// `read` and `write` take `&mut self` because chunked mode loads data lazily.
pub trait FileData: Send {
    fn read(&mut self, offset: usize, size: usize) -> Result<Vec<u8>, i32>;
    fn write(&mut self, offset: usize, data: &[u8]) -> Result<usize, i32>;
    fn truncate(&mut self, new_size: usize);
    fn size(&self) -> i64;
    fn is_dirty(&self) -> bool;
    fn flush_to_ipfs(&mut self, mdf: &Path) -> Result<i64, i32>;
}

/// Create a new empty file data for the given storage mode.
pub fn new_file(mode: StorageMode) -> Box<dyn FileData> {
    match mode {
        StorageMode::WholeFile => Box::new(WholeFileData::new()),
        StorageMode::Chunked => Box::new(ChunkedFileData::new()),
    }
}

/// Open an existing file from its MDD hashes.
pub fn open_file(
    mode: StorageMode,
    hashes: Vec<String>,
    size_hint: Option<i64>,
) -> Result<Box<dyn FileData>, i32> {
    match mode {
        StorageMode::WholeFile => {
            if hashes.is_empty() {
                Ok(Box::new(WholeFileData::new()))
            } else {
                let data = ipfs::ipfs_cat(&hashes)?;
                Ok(Box::new(WholeFileData::with_data(data)))
            }
        }
        StorageMode::Chunked => {
            Ok(Box::new(ChunkedFileData::from_hashes(hashes, size_hint)))
        }
    }
}

/// Set an xattr on a file.
fn set_xattr(path: &Path, name: &str, value: &[u8]) {
    let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
    let c_name = CString::new(name).unwrap();

    #[cfg(target_os = "macos")]
    let ret = unsafe {
        libc::setxattr(
            c_path.as_ptr(),
            c_name.as_ptr(),
            value.as_ptr() as *const libc::c_void,
            value.len(),
            0,
            0,
        )
    };
    #[cfg(target_os = "linux")]
    let ret = unsafe {
        libc::setxattr(
            c_path.as_ptr(),
            c_name.as_ptr(),
            value.as_ptr() as *const libc::c_void,
            value.len(),
            0,
        )
    };

    if ret == -1 {
        log::warn!("failed to set xattr {} on {:?}: {}", name, path, std::io::Error::last_os_error());
    }
}

/// Remove an xattr from a file (ignore errors).
fn remove_xattr(path: &Path, name: &str) {
    let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
    let c_name = CString::new(name).unwrap();

    #[cfg(target_os = "macos")]
    unsafe { libc::removexattr(c_path.as_ptr(), c_name.as_ptr(), libc::XATTR_NOFOLLOW); }
    #[cfg(target_os = "linux")]
    unsafe { libc::lremovexattr(c_path.as_ptr(), c_name.as_ptr()); }
}

/// Read an xattr value from a file. Returns None if not found.
pub fn get_xattr(path: &Path, name: &str) -> Option<Vec<u8>> {
    let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
    let c_name = CString::new(name).unwrap();

    // First get the size
    #[cfg(target_os = "macos")]
    let size = unsafe {
        libc::getxattr(c_path.as_ptr(), c_name.as_ptr(), std::ptr::null_mut(), 0, 0, libc::XATTR_NOFOLLOW)
    };
    #[cfg(target_os = "linux")]
    let size = unsafe {
        libc::lgetxattr(c_path.as_ptr(), c_name.as_ptr(), std::ptr::null_mut(), 0)
    };

    if size < 0 {
        return None;
    }

    let mut buf = vec![0u8; size as usize];
    #[cfg(target_os = "macos")]
    let ret = unsafe {
        libc::getxattr(c_path.as_ptr(), c_name.as_ptr(), buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0, libc::XATTR_NOFOLLOW)
    };
    #[cfg(target_os = "linux")]
    let ret = unsafe {
        libc::lgetxattr(c_path.as_ptr(), c_name.as_ptr(), buf.as_mut_ptr() as *mut libc::c_void, buf.len())
    };

    if ret < 0 {
        return None;
    }
    buf.truncate(ret as usize);
    Some(buf)
}

/// Set `ipfs.size` xattr on the MDD file.
pub fn set_ipfs_size_xattr(mdf: &Path, size: i64) {
    set_xattr(mdf, XATTR_IPFS_SIZE, size.to_string().as_bytes());
}

/// Read `ipfs.size` xattr from the MDD file.
pub fn get_ipfs_size_xattr(mdf: &Path) -> Option<i64> {
    let val = get_xattr(mdf, XATTR_IPFS_SIZE)?;
    String::from_utf8(val).ok()?.trim().parse().ok()
}

/// Compute the whole-file IPFS hash from chunk hashes in the MDD file.
/// Reads all chunks from IPFS, concatenates, and does `ipfs add` to get the single hash.
/// Sets the result as xattr on the MDD file for caching.
pub fn compute_ipfs_hash(mdf: &Path, hashes: &[String], total_size: i64) -> Result<String, i32> {
    let total_bytes = total_size as usize;
    if total_bytes == 0 || hashes.is_empty() {
        return Err(libc::ENODATA);
    }
    let data = ipfs::ipfs_cat(hashes)?;
    let data = &data[..total_bytes.min(data.len())];
    let hash = ipfs::ipfs_add(data)?;
    set_xattr(mdf, XATTR_IPFS_HASH, hash.as_bytes());
    Ok(hash)
}

// ─── WholeFileData ──────────────────────────────────────────

struct WholeFileData {
    data: Vec<u8>,
    dirty: bool,
    write_frontier: usize,
}

impl WholeFileData {
    fn new() -> Self {
        WholeFileData {
            data: Vec::new(),
            dirty: false,
            write_frontier: 0,
        }
    }

    fn with_data(data: Vec<u8>) -> Self {
        WholeFileData {
            data,
            dirty: false,
            write_frontier: 0,
        }
    }
}

impl FileData for WholeFileData {
    fn read(&mut self, offset: usize, size: usize) -> Result<Vec<u8>, i32> {
        if offset >= self.data.len() {
            return Ok(Vec::new());
        }
        let end = std::cmp::min(offset + size, self.data.len());
        Ok(self.data[offset..end].to_vec())
    }

    fn write(&mut self, offset: usize, data: &[u8]) -> Result<usize, i32> {
        let end = offset + data.len();

        // Detect kernel page cache replays: if this write is below the
        // write frontier and the buffer already has data there, the kernel
        // is replaying stale cached pages. Skip to preserve correct data.
        if offset < self.write_frontier && end <= self.data.len() {
            return Ok(data.len());
        }

        if end > self.data.len() {
            self.data.resize(end, 0);
        }
        self.data[offset..end].copy_from_slice(data);
        if end > self.write_frontier {
            self.write_frontier = end;
        }
        self.dirty = true;
        Ok(data.len())
    }

    fn truncate(&mut self, new_size: usize) {
        self.data.resize(new_size, 0);
        self.dirty = true;
        self.write_frontier = new_size;
    }

    fn size(&self) -> i64 {
        self.data.len() as i64
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn flush_to_ipfs(&mut self, mdf: &Path) -> Result<i64, i32> {
        if !self.dirty {
            return Ok(self.data.len() as i64);
        }
        let hash = ipfs::ipfs_add(&self.data)?;
        std::fs::write(mdf, format!("{}\n", hash)).map_err(|e| {
            log::error!("failed to write hash to {:?}: {}", mdf, e);
            libc::EIO
        })?;
        set_xattr(mdf, XATTR_IPFS_HASH, hash.as_bytes());
        let size = self.data.len() as i64;
        set_ipfs_size_xattr(mdf, size);
        self.dirty = false;
        Ok(size)
    }
}

// ─── ChunkedFileData ────────────────────────────────────────

struct ChunkedFileData {
    /// Loaded chunk data, indexed by chunk number.
    chunks: HashMap<usize, Vec<u8>>,
    /// IPFS hash for each chunk (from MDD file).
    hashes: Vec<String>,
    /// Which chunks have been modified and need re-upload.
    dirty_chunks: HashSet<usize>,
    /// Total logical file size.
    total_size: i64,
    /// Highest byte offset written — used to detect kernel page cache replays.
    /// macFUSE replays stale cached pages at previously-written offsets even
    /// with FOPEN_DIRECT_IO set.
    write_frontier: usize,
}

impl ChunkedFileData {
    fn new() -> Self {
        ChunkedFileData {
            chunks: HashMap::new(),
            hashes: Vec::new(),
            dirty_chunks: HashSet::new(),
            total_size: 0,
            write_frontier: 0,
        }
    }

    fn from_hashes(hashes: Vec<String>, size_hint: Option<i64>) -> Self {
        let total_size = if let Some(s) = size_hint {
            s
        } else if hashes.is_empty() {
            0
        } else {
            // We need to know the total size. Sum up sizes from IPFS.
            let mut total: i64 = 0;
            for h in &hashes {
                match ipfs::ipfs_file_size(h) {
                    Ok(s) => total += s,
                    Err(_) => {
                        log::error!("failed to get size for hash {}", h);
                        break;
                    }
                }
            }
            total
        };
        ChunkedFileData {
            chunks: HashMap::new(),
            hashes,
            dirty_chunks: HashSet::new(),
            total_size,
            write_frontier: 0,
        }
    }

    /// Ensure chunk `idx` is loaded into memory. If it has an IPFS hash
    /// and hasn't been loaded yet, fetch it.
    fn ensure_chunk(&mut self, idx: usize) -> Result<(), i32> {
        if self.chunks.contains_key(&idx) {
            return Ok(());
        }
        if idx < self.hashes.len() {
            let data = ipfs::ipfs_cat(&[self.hashes[idx].clone()])?;
            self.chunks.insert(idx, data);
        } else {
            // Beyond known hashes — empty chunk
            self.chunks.insert(idx, Vec::new());
        }
        Ok(())
    }

    /// Number of chunks needed to cover `total_size` bytes.
    fn num_chunks(&self) -> usize {
        if self.total_size <= 0 {
            0
        } else {
            ((self.total_size as usize) + CHUNK_SIZE - 1) / CHUNK_SIZE
        }
    }
}

impl FileData for ChunkedFileData {
    fn read(&mut self, offset: usize, size: usize) -> Result<Vec<u8>, i32> {
        if offset as i64 >= self.total_size {
            return Ok(Vec::new());
        }
        let end = std::cmp::min(offset + size, self.total_size as usize);
        let mut result = Vec::with_capacity(end - offset);

        let start_chunk = offset / CHUNK_SIZE;
        let end_chunk = (end - 1) / CHUNK_SIZE;

        for cidx in start_chunk..=end_chunk {
            self.ensure_chunk(cidx)?;
            let chunk = self.chunks.get(&cidx).unwrap();

            let chunk_start = cidx * CHUNK_SIZE;
            let local_start = if cidx == start_chunk {
                offset - chunk_start
            } else {
                0
            };
            let local_end = if cidx == end_chunk {
                end - chunk_start
            } else {
                chunk.len()
            };

            // Clamp to actual chunk data length
            let local_end = std::cmp::min(local_end, chunk.len());
            if local_start < local_end {
                result.extend_from_slice(&chunk[local_start..local_end]);
            }
        }

        Ok(result)
    }

    fn write(&mut self, offset: usize, data: &[u8]) -> Result<usize, i32> {
        let end = offset + data.len();

        // Detect kernel page cache replays: if this write is entirely below
        // the write frontier, the kernel is replaying stale cached pages.
        // Skip to preserve the correct data written in the first pass.
        if offset < self.write_frontier && end <= self.total_size as usize {
            return Ok(data.len());
        }

        let start_chunk = offset / CHUNK_SIZE;
        let end_chunk = if data.is_empty() {
            start_chunk
        } else {
            (end - 1) / CHUNK_SIZE
        };

        let mut data_pos = 0;
        for cidx in start_chunk..=end_chunk {
            self.ensure_chunk(cidx)?;
            let chunk = self.chunks.get_mut(&cidx).unwrap();

            let chunk_start = cidx * CHUNK_SIZE;
            let local_start = if cidx == start_chunk {
                offset - chunk_start
            } else {
                0
            };
            let local_end = if cidx == end_chunk {
                end - chunk_start
            } else {
                CHUNK_SIZE
            };

            // Extend chunk if needed
            if local_end > chunk.len() {
                chunk.resize(local_end, 0);
            }

            let write_len = local_end - local_start;
            chunk[local_start..local_end].copy_from_slice(&data[data_pos..data_pos + write_len]);
            data_pos += write_len;

            self.dirty_chunks.insert(cidx);
        }

        if end > self.write_frontier {
            self.write_frontier = end;
        }
        if (end as i64) > self.total_size {
            self.total_size = end as i64;
        }

        Ok(data.len())
    }

    fn truncate(&mut self, new_size: usize) {
        self.total_size = new_size as i64;
        let needed_chunks = if new_size == 0 {
            0
        } else {
            (new_size + CHUNK_SIZE - 1) / CHUNK_SIZE
        };

        // Remove chunks beyond the new size
        let to_remove: Vec<usize> = self
            .chunks
            .keys()
            .filter(|&&k| k >= needed_chunks)
            .copied()
            .collect();
        for k in to_remove {
            self.chunks.remove(&k);
            self.dirty_chunks.remove(&k);
        }

        // Truncate hashes list
        if self.hashes.len() > needed_chunks {
            self.hashes.truncate(needed_chunks);
        }

        // Truncate last chunk if loaded
        if needed_chunks > 0 {
            let last_idx = needed_chunks - 1;
            let last_chunk_size = new_size - last_idx * CHUNK_SIZE;
            if let Some(chunk) = self.chunks.get_mut(&last_idx) {
                if chunk.len() > last_chunk_size {
                    chunk.truncate(last_chunk_size);
                    self.dirty_chunks.insert(last_idx);
                }
            }
        }

        // Mark all remaining loaded chunks as dirty since we're changing the file
        for cidx in self.chunks.keys() {
            self.dirty_chunks.insert(*cidx);
        }

        self.write_frontier = new_size;
    }

    fn size(&self) -> i64 {
        self.total_size
    }

    fn is_dirty(&self) -> bool {
        !self.dirty_chunks.is_empty()
    }

    fn flush_to_ipfs(&mut self, mdf: &Path) -> Result<i64, i32> {
        if self.dirty_chunks.is_empty() {
            return Ok(self.total_size);
        }

        let num_chunks = self.num_chunks();

        // Ensure hashes vec is big enough
        if self.hashes.len() < num_chunks {
            self.hashes.resize(num_chunks, String::new());
        }
        // Trim if file was truncated
        if self.hashes.len() > num_chunks {
            self.hashes.truncate(num_chunks);
        }

        // Upload dirty chunks
        for &cidx in &self.dirty_chunks.clone() {
            if cidx >= num_chunks {
                continue;
            }
            if let Some(chunk) = self.chunks.get(&cidx) {
                // If chunk is larger than CHUNK_SIZE, split it
                if chunk.len() > CHUNK_SIZE {
                    // Re-chunk: split into CHUNK_SIZE pieces, upload each
                    let sub_chunks: Vec<&[u8]> = chunk.chunks(CHUNK_SIZE).collect();
                    // This case shouldn't normally happen with correct writes,
                    // but handle it for cross-mode compatibility.
                    // Upload first sub-chunk as this chunk index
                    let hash = ipfs::ipfs_add(sub_chunks[0])?;
                    self.hashes[cidx] = hash;
                    // Insert additional chunks — shift everything after
                    for (i, sub) in sub_chunks.iter().enumerate().skip(1) {
                        let new_hash = ipfs::ipfs_add(sub)?;
                        self.hashes.insert(cidx + i, new_hash);
                    }
                } else {
                    let hash = ipfs::ipfs_add(chunk)?;
                    self.hashes[cidx] = hash;
                }
            }
        }

        // Write all hashes to MDD file
        let content: String = self
            .hashes
            .iter()
            .map(|h| format!("{}\n", h))
            .collect();
        std::fs::write(mdf, content).map_err(|e| {
            log::error!("failed to write hashes to {:?}: {}", mdf, e);
            libc::EIO
        })?;

        // Invalidate cached whole-file hash (will be recomputed lazily on getxattr)
        remove_xattr(mdf, XATTR_IPFS_HASH);

        // Store file size as xattr (avoids ipfs files stat calls on next mount)
        set_ipfs_size_xattr(mdf, self.total_size);

        self.dirty_chunks.clear();
        Ok(self.total_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── WholeFileData tests ─────────────────────────────────

    #[test]
    fn test_whole_file_new_is_empty() {
        let mut fd = WholeFileData::new();
        assert_eq!(fd.size(), 0);
        assert!(!fd.is_dirty());
        assert_eq!(fd.read(0, 100).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn test_whole_file_write_and_read() {
        let mut fd = WholeFileData::new();
        fd.write(0, b"hello").unwrap();
        assert_eq!(fd.size(), 5);
        assert!(fd.is_dirty());
        assert_eq!(fd.read(0, 5).unwrap(), b"hello");
    }

    #[test]
    fn test_whole_file_write_extending_past_frontier() {
        let mut fd = WholeFileData::new();
        fd.write(0, b"hello").unwrap();
        // Write that extends past the frontier is accepted
        fd.write(3, b"LO WORLD").unwrap();
        assert_eq!(fd.read(0, 11).unwrap(), b"helLO WORLD");
    }

    #[test]
    fn test_whole_file_write_frontier_skips_replays() {
        let mut fd = WholeFileData::new();
        fd.write(0, b"AAAA").unwrap();
        // Simulate kernel replay at offset 0 — should be skipped
        fd.write(0, b"BBBB").unwrap();
        assert_eq!(fd.read(0, 4).unwrap(), b"AAAA");
    }

    #[test]
    fn test_whole_file_truncate() {
        let mut fd = WholeFileData::with_data(b"hello world".to_vec());
        fd.truncate(5);
        assert_eq!(fd.size(), 5);
        assert!(fd.is_dirty());
        assert_eq!(fd.read(0, 10).unwrap(), b"hello");
    }

    #[test]
    fn test_whole_file_truncate_extend() {
        let mut fd = WholeFileData::with_data(b"hi".to_vec());
        fd.truncate(5);
        assert_eq!(fd.size(), 5);
        assert_eq!(fd.read(0, 5).unwrap(), b"hi\0\0\0");
    }

    #[test]
    fn test_whole_file_read_past_end() {
        let fd = WholeFileData::with_data(b"abc".to_vec());
        let mut fd: Box<dyn FileData> = Box::new(fd);
        assert_eq!(fd.read(10, 5).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn test_whole_file_read_clamped() {
        let fd = WholeFileData::with_data(b"abc".to_vec());
        let mut fd: Box<dyn FileData> = Box::new(fd);
        assert_eq!(fd.read(1, 100).unwrap(), b"bc");
    }

    // ─── ChunkedFileData tests ───────────────────────────────

    #[test]
    fn test_chunked_new_is_empty() {
        let mut fd = ChunkedFileData::new();
        assert_eq!(fd.size(), 0);
        assert!(!fd.is_dirty());
        assert_eq!(fd.read(0, 100).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn test_chunked_write_and_read() {
        let mut fd = ChunkedFileData::new();
        fd.write(0, b"hello").unwrap();
        assert_eq!(fd.size(), 5);
        assert!(fd.is_dirty());
        assert_eq!(fd.read(0, 5).unwrap(), b"hello");
    }

    #[test]
    fn test_chunked_write_across_chunk_boundary() {
        let mut fd = ChunkedFileData::new();
        // Write data that spans two chunks
        let offset = CHUNK_SIZE - 3;
        fd.write(offset, b"ABCDEF").unwrap();
        assert_eq!(fd.size(), (offset + 6) as i64);
        assert_eq!(fd.read(offset, 6).unwrap(), b"ABCDEF");
        assert!(fd.dirty_chunks.contains(&0));
        assert!(fd.dirty_chunks.contains(&1));
    }

    #[test]
    fn test_chunked_truncate_shrink() {
        let mut fd = ChunkedFileData::new();
        fd.write(0, b"hello world").unwrap();
        fd.truncate(5);
        assert_eq!(fd.size(), 5);
        assert_eq!(fd.read(0, 10).unwrap(), b"hello");
    }

    #[test]
    fn test_chunked_truncate_to_zero() {
        let mut fd = ChunkedFileData::new();
        fd.write(0, b"hello").unwrap();
        fd.truncate(0);
        assert_eq!(fd.size(), 0);
        assert_eq!(fd.read(0, 10).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn test_chunked_write_at_offset_with_gap() {
        let mut fd = ChunkedFileData::new();
        fd.write(10, b"hello").unwrap();
        assert_eq!(fd.size(), 15);
        // Bytes 0-9 should be zero
        let data = fd.read(0, 15).unwrap();
        assert_eq!(&data[..10], &[0u8; 10]);
        assert_eq!(&data[10..], b"hello");
    }
}
