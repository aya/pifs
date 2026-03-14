use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum GitEvent {
    FileChanged(PathBuf),
    FileDeleted(PathBuf),
    DirCreated(PathBuf),
    DirDeleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
    MetadataChanged(PathBuf),
    Shutdown,
}

pub struct GitSync {
    sender: mpsc::Sender<GitEvent>,
    handle: Option<JoinHandle<()>>,
}

impl GitSync {
    /// Initialize git in the MDD and spawn the background commit thread.
    pub fn new(mdd: PathBuf) -> Result<Self, String> {
        git_init(&mdd)?;
        let (tx, rx) = mpsc::channel();
        let mdd_clone = mdd.clone();
        let handle = thread::Builder::new()
            .name("git-sync".to_string())
            .spawn(move || background_loop(mdd_clone, rx))
            .map_err(|e| format!("failed to spawn git-sync thread: {}", e))?;
        Ok(GitSync {
            sender: tx,
            handle: Some(handle),
        })
    }

    /// Send an event to the background thread (fire-and-forget).
    pub fn send(&self, event: GitEvent) {
        let _ = self.sender.send(event);
    }

    /// Send a GitEvent, converting an absolute path to relative to the MDD.
    pub fn notify(&self, mdd: &Path, event: GitEvent) {
        let event = relativize_event(mdd, event);
        self.send(event);
    }

    /// Flush pending events and shut down the background thread.
    pub fn shutdown(&mut self) {
        let _ = self.sender.send(GitEvent::Shutdown);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for GitSync {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Convert absolute paths in a GitEvent to paths relative to `mdd`.
fn relativize_event(mdd: &Path, event: GitEvent) -> GitEvent {
    let rel = |p: PathBuf| -> PathBuf {
        p.strip_prefix(mdd).map(|r| r.to_path_buf()).unwrap_or(p)
    };
    match event {
        GitEvent::FileChanged(p) => GitEvent::FileChanged(rel(p)),
        GitEvent::FileDeleted(p) => GitEvent::FileDeleted(rel(p)),
        GitEvent::DirCreated(p) => GitEvent::DirCreated(rel(p)),
        GitEvent::DirDeleted(p) => GitEvent::DirDeleted(rel(p)),
        GitEvent::Renamed { from, to } => GitEvent::Renamed {
            from: rel(from),
            to: rel(to),
        },
        GitEvent::MetadataChanged(p) => GitEvent::MetadataChanged(rel(p)),
        GitEvent::Shutdown => GitEvent::Shutdown,
    }
}

/// Initialize a git repo in the MDD if one doesn't exist.
fn git_init(mdd: &Path) -> Result<(), String> {
    let git_dir = mdd.join(".git");
    if git_dir.is_dir() {
        return Ok(());
    }

    run_git(mdd, &["init"])?;
    // Configure user for commits (local to this repo only)
    run_git(mdd, &["config", "user.email", "pifs@localhost"])?;
    run_git(mdd, &["config", "user.name", "pifs"])?;

    // Initial commit (allow empty)
    run_git(mdd, &["commit", "--allow-empty", "-m", "pifs: initial commit"])?;
    Ok(())
}

/// Run a git command in the given directory. Returns stdout on success.
fn run_git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git {}: {}", args.join(" "), e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {} failed: {}", args.join(" "), stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Background loop: accumulate events, debounce 2s, then commit.
fn background_loop(mdd: PathBuf, rx: mpsc::Receiver<GitEvent>) {
    let mut pending: Vec<GitEvent> = Vec::new();
    loop {
        let event = if pending.is_empty() {
            // Block until an event arrives
            match rx.recv() {
                Ok(ev) => ev,
                Err(_) => break, // channel closed
            }
        } else {
            // Wait up to 2s for another event (debounce)
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(ev) => ev,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    commit_pending(&mdd, &pending);
                    pending.clear();
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    commit_pending(&mdd, &pending);
                    break;
                }
            }
        };
        match event {
            GitEvent::Shutdown => {
                commit_pending(&mdd, &pending);
                break;
            }
            other => pending.push(other),
        }
    }
}

/// Stage all changes and commit with a descriptive message.
fn commit_pending(mdd: &Path, events: &[GitEvent]) {
    if events.is_empty() {
        return;
    }

    // Stage all changes
    if run_git(mdd, &["add", "-A"]).is_err() {
        return;
    }

    // Check if there's anything to commit
    if run_git(mdd, &["diff", "--cached", "--quiet"]).is_ok() {
        return; // nothing staged
    }

    let msg = build_commit_message(events);
    let _ = run_git(mdd, &["commit", "-m", &msg]);
}

/// Build a commit message summarizing the events.
pub fn build_commit_message(events: &[GitEvent]) -> String {
    let mut lines = Vec::new();
    for ev in events {
        match ev {
            GitEvent::FileChanged(p) => lines.push(format!("  modified: {}", p.display())),
            GitEvent::FileDeleted(p) => lines.push(format!("  deleted: {}", p.display())),
            GitEvent::DirCreated(p) => lines.push(format!("  mkdir: {}", p.display())),
            GitEvent::DirDeleted(p) => lines.push(format!("  rmdir: {}", p.display())),
            GitEvent::Renamed { from, to } => {
                lines.push(format!("  renamed: {} -> {}", from.display(), to.display()))
            }
            GitEvent::MetadataChanged(p) => {
                lines.push(format!("  metadata: {}", p.display()))
            }
            GitEvent::Shutdown => {}
        }
    }
    if lines.is_empty() {
        "pifs: auto-commit MDD changes".to_string()
    } else {
        format!("pifs: auto-commit MDD changes\n\n{}", lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pifs-git-test-{}", std::process::id()));
        let dir = dir.join(format!("{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git_log_count(mdd: &Path) -> usize {
        let out = run_git(mdd, &["rev-list", "--count", "HEAD"]).unwrap();
        out.trim().parse().unwrap()
    }

    fn git_log_last_message(mdd: &Path) -> String {
        run_git(mdd, &["log", "-1", "--format=%B"]).unwrap()
    }

    #[test]
    fn test_git_init_creates_repo() {
        let mdd = temp_dir();
        git_init(&mdd).unwrap();
        assert!(mdd.join(".git").is_dir());
        // Should have exactly 1 commit (the initial)
        assert_eq!(git_log_count(&mdd), 1);
        fs::remove_dir_all(&mdd).unwrap();
    }

    #[test]
    fn test_git_init_skips_existing_repo() {
        let mdd = temp_dir();
        git_init(&mdd).unwrap();
        // Second call should succeed without error
        git_init(&mdd).unwrap();
        // Still only 1 commit
        assert_eq!(git_log_count(&mdd), 1);
        fs::remove_dir_all(&mdd).unwrap();
    }

    #[test]
    fn test_single_file_change_commits() {
        let mdd = temp_dir();
        let mut gs = GitSync::new(mdd.clone()).unwrap();

        // Create a file in the MDD
        fs::write(mdd.join("test.txt"), "hello\n").unwrap();

        // Send event and shut down (forces flush)
        gs.send(GitEvent::FileChanged(PathBuf::from("test.txt")));
        gs.shutdown();

        // Should have 2 commits: initial + the file change
        assert_eq!(git_log_count(&mdd), 2);
        fs::remove_dir_all(&mdd).unwrap();
    }

    #[test]
    fn test_debounce_batches_events() {
        let mdd = temp_dir();
        let mut gs = GitSync::new(mdd.clone()).unwrap();

        // Rapidly send 5 events
        for i in 0..5 {
            let name = format!("file{}.txt", i);
            fs::write(mdd.join(&name), format!("content {}\n", i)).unwrap();
            gs.send(GitEvent::FileChanged(PathBuf::from(&name)));
        }

        gs.shutdown();

        // Should have 2 commits: initial + 1 batched commit (all 5 events)
        assert_eq!(git_log_count(&mdd), 2);
        fs::remove_dir_all(&mdd).unwrap();
    }

    #[test]
    fn test_shutdown_flushes_pending() {
        let mdd = temp_dir();
        let mut gs = GitSync::new(mdd.clone()).unwrap();

        fs::write(mdd.join("pending.txt"), "data\n").unwrap();
        gs.send(GitEvent::FileChanged(PathBuf::from("pending.txt")));

        // Shutdown should flush without waiting for debounce timeout
        gs.shutdown();

        assert_eq!(git_log_count(&mdd), 2);
        fs::remove_dir_all(&mdd).unwrap();
    }

    #[test]
    fn test_no_commit_when_nothing_changed() {
        let mdd = temp_dir();
        let mut gs = GitSync::new(mdd.clone()).unwrap();

        // Send event but don't actually change any files
        gs.send(GitEvent::FileChanged(PathBuf::from("phantom.txt")));
        gs.shutdown();

        // Should still have only the initial commit
        assert_eq!(git_log_count(&mdd), 1);
        fs::remove_dir_all(&mdd).unwrap();
    }

    #[test]
    fn test_commit_message_format() {
        let events = vec![
            GitEvent::FileChanged(PathBuf::from("foo.txt")),
            GitEvent::FileDeleted(PathBuf::from("bar.txt")),
        ];
        let msg = build_commit_message(&events);
        assert!(msg.starts_with("pifs: auto-commit MDD changes"));
        assert!(msg.contains("modified: foo.txt"));
        assert!(msg.contains("deleted: bar.txt"));
    }

    #[test]
    fn test_commit_message_rename() {
        let events = vec![GitEvent::Renamed {
            from: PathBuf::from("old.txt"),
            to: PathBuf::from("new.txt"),
        }];
        let msg = build_commit_message(&events);
        assert!(msg.contains("renamed: old.txt -> new.txt"));
    }

    #[test]
    fn test_delete_event() {
        let mdd = temp_dir();
        let mut gs = GitSync::new(mdd.clone()).unwrap();

        // Create then delete a file
        let f = mdd.join("to_delete.txt");
        fs::write(&f, "bye\n").unwrap();
        // First, commit the creation
        gs.send(GitEvent::FileChanged(PathBuf::from("to_delete.txt")));
        gs.shutdown();

        // Now delete
        let mut gs2 = GitSync::new(mdd.clone()).unwrap();
        fs::remove_file(&f).unwrap();
        gs2.send(GitEvent::FileDeleted(PathBuf::from("to_delete.txt")));
        gs2.shutdown();

        let msg = git_log_last_message(&mdd);
        assert!(msg.contains("deleted: to_delete.txt"));
        fs::remove_dir_all(&mdd).unwrap();
    }

    #[test]
    fn test_drop_calls_shutdown() {
        let mdd = temp_dir();

        {
            let gs = GitSync::new(mdd.clone()).unwrap();
            fs::write(mdd.join("drop_test.txt"), "drop\n").unwrap();
            gs.send(GitEvent::FileChanged(PathBuf::from("drop_test.txt")));
            // gs is dropped here
        }

        // Give the thread a moment to finish after drop
        std::thread::sleep(Duration::from_millis(100));

        // The file change should have been committed on drop
        assert_eq!(git_log_count(&mdd), 2);
        fs::remove_dir_all(&mdd).unwrap();
    }

    #[test]
    fn test_relativize_event() {
        let mdd = PathBuf::from("/tmp/mdd");
        let ev = GitEvent::FileChanged(PathBuf::from("/tmp/mdd/subdir/file.txt"));
        let rel = relativize_event(&mdd, ev);
        match rel {
            GitEvent::FileChanged(p) => assert_eq!(p, PathBuf::from("subdir/file.txt")),
            _ => panic!("expected FileChanged"),
        }
    }

    #[test]
    fn test_relativize_event_already_relative() {
        let mdd = PathBuf::from("/tmp/mdd");
        let ev = GitEvent::FileChanged(PathBuf::from("already/relative.txt"));
        let rel = relativize_event(&mdd, ev);
        match rel {
            GitEvent::FileChanged(p) => assert_eq!(p, PathBuf::from("already/relative.txt")),
            _ => panic!("expected FileChanged"),
        }
    }
}
