use std::process::{Command, Stdio};

/// Run `ipfs cat <hash>` for each hash and concatenate the results.
pub fn ipfs_cat(hashes: &[String]) -> Result<Vec<u8>, i32> {
    let mut data = Vec::new();
    for hash in hashes {
        let output = Command::new("ipfs")
            .args(["cat", hash])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                log::error!("failed to run ipfs cat: {}", e);
                libc::EIO
            })?;

        if !output.status.success() {
            log::error!("ipfs cat {} failed: {}", hash, String::from_utf8_lossy(&output.stderr));
            return Err(libc::EIO);
        }
        data.extend_from_slice(&output.stdout);
    }
    Ok(data)
}

/// Run `ipfs add -Q <tmpfile>` to add data to IPFS.
/// Uses a temporary file to avoid pipe deadlocks with large data.
/// Returns the IPFS hash (CID).
pub fn ipfs_add(data: &[u8]) -> Result<String, i32> {
    // Write data to a temporary file
    let tmp_dir = std::env::temp_dir();
    let tmp_path = tmp_dir.join(format!("pifs-add-{}", std::process::id()));

    std::fs::write(&tmp_path, data).map_err(|e| {
        log::error!("failed to write temp file {:?}: {}", tmp_path, e);
        libc::EIO
    })?;

    let output = Command::new("ipfs")
        .args(["add", "-Q"])
        .arg(&tmp_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            log::error!("failed to run ipfs add: {}", e);
            let _ = std::fs::remove_file(&tmp_path);
            libc::EIO
        })?;

    let _ = std::fs::remove_file(&tmp_path);

    if !output.status.success() {
        log::error!("ipfs add failed: {}", String::from_utf8_lossy(&output.stderr));
        return Err(libc::EIO);
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(hash)
}

/// Run `ipfs files stat /ipfs/<hash>` and parse the Size field.
pub fn ipfs_file_size(hash: &str) -> Result<i64, i32> {
    let output = Command::new("ipfs")
        .args(["files", "stat", &format!("/ipfs/{}", hash)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            log::error!("failed to run ipfs files stat: {}", e);
            libc::EIO
        })?;

    if !output.status.success() {
        log::error!(
            "ipfs files stat failed for {}: {}",
            hash,
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(libc::EIO);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0] == "Size:" {
            return parts[1].parse::<i64>().map_err(|_| libc::EIO);
        }
    }
    Err(libc::EIO)
}
