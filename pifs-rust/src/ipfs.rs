use std::io::Write;
use std::process::{Command, Stdio};

/// Run `ipfs cat` with the given hashes (one per line) piped to stdin.
/// Returns the concatenated file data.
pub fn ipfs_cat(hashes: &[String]) -> Result<Vec<u8>, i32> {
    let input = hashes.join("\n");
    let mut child = Command::new("ipfs")
        .arg("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| libc::EIO)?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(input.as_bytes()).map_err(|_| libc::EIO)?;
    }
    // Close stdin so ipfs cat can process
    drop(child.stdin.take());

    let output = child.wait_with_output().map_err(|_| libc::EIO)?;
    if !output.status.success() {
        log::error!("ipfs cat failed: {}", String::from_utf8_lossy(&output.stderr));
        return Err(libc::EIO);
    }
    Ok(output.stdout)
}

/// Run `ipfs add -Q` with data piped to stdin.
/// Returns the IPFS hash (CID).
pub fn ipfs_add(data: &[u8]) -> Result<String, i32> {
    let mut child = Command::new("ipfs")
        .args(["add", "-Q"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| libc::EIO)?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(data).map_err(|_| libc::EIO)?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().map_err(|_| libc::EIO)?;
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
        .map_err(|_| libc::EIO)?;

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
