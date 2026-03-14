use std::collections::HashMap;
use std::path::PathBuf;

use crate::file_data::{FileData, StorageMode};

pub const PIFS_VERSION: &str = "0.1.0";
pub const HASH_SIZE: usize = 47;

/// The main filesystem state.
pub struct PifsFilesystem {
    /// Metadata directory path (stores IPFS hashes).
    pub mdd: PathBuf,
    /// Log file path (optional).
    pub log_file: Option<PathBuf>,
    /// Open files indexed by inode.
    pub files: HashMap<u64, Box<dyn FileData>>,
    /// Cached file sizes indexed by inode (persists across open/close).
    pub size_cache: HashMap<u64, i64>,
    /// Inode to metadata path mapping.
    pub inode_paths: HashMap<u64, PathBuf>,
    /// Storage mode for file data.
    pub storage_mode: StorageMode,
}

impl PifsFilesystem {
    pub fn new(mdd: PathBuf, log_file: Option<PathBuf>, storage_mode: StorageMode) -> Self {
        PifsFilesystem {
            mdd,
            log_file,
            files: HashMap::new(),
            size_cache: HashMap::new(),
            inode_paths: HashMap::new(),
            storage_mode,
        }
    }

    /// Build the metadata file path: mdd + fuse_path.
    pub fn mdf_path(&self, path: &std::path::Path) -> PathBuf {
        let rel = path.strip_prefix("/").unwrap_or(path);
        self.mdd.join(rel)
    }

    /// Get the inode of a metadata file.
    pub fn mdf_inode(&self, mdf_path: &std::path::Path) -> Option<u64> {
        use std::os::unix::fs::MetadataExt;
        std::fs::symlink_metadata(mdf_path)
            .map(|m| m.ino())
            .ok()
    }
}
