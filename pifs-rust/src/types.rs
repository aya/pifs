use std::collections::HashMap;
use std::path::PathBuf;

pub const PIFS_VERSION: &str = "0.1.0";
pub const HASH_SIZE: usize = 47;

/// In-memory representation of an open file's data.
pub struct PifsFile {
    pub data: Vec<u8>,
    pub size: i64,
    /// Track whether the file was written to (needs ipfs add on release).
    pub dirty: bool,
}

impl PifsFile {
    pub fn new() -> Self {
        PifsFile {
            data: Vec::new(),
            size: 0,
            dirty: false,
        }
    }

    pub fn with_data(data: Vec<u8>) -> Self {
        let size = data.len() as i64;
        PifsFile {
            data,
            size,
            dirty: false,
        }
    }
}

/// The main filesystem state.
pub struct PifsFilesystem {
    /// Metadata directory path (stores IPFS hashes).
    pub mdd: PathBuf,
    /// Log file path (optional).
    pub log_file: Option<PathBuf>,
    /// Open files indexed by inode.
    pub files: HashMap<u64, PifsFile>,
    /// Cached file sizes indexed by inode (persists across open/close).
    pub size_cache: HashMap<u64, i64>,
    /// Inode to metadata path mapping.
    pub inode_paths: HashMap<u64, PathBuf>,
}

impl PifsFilesystem {
    pub fn new(mdd: PathBuf, log_file: Option<PathBuf>) -> Self {
        PifsFilesystem {
            mdd,
            log_file,
            files: HashMap::new(),
            size_cache: HashMap::new(),
            inode_paths: HashMap::new(),
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
