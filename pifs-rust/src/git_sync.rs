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
    /// Initialize git in a separate git_dir with the MDD as work tree, and spawn the background commit thread.
    pub fn new(mdd: PathBuf, git_dir: PathBuf) -> Result<Self, String> {
        git_init(&mdd, &git_dir)?;
        let (tx, rx) = mpsc::channel();
        let mdd_clone = mdd.clone();
        let git_dir_clone = git_dir.clone();
        let handle = thread::Builder::new()
            .name("git-sync".to_string())
            .spawn(move || background_loop(mdd_clone, git_dir_clone, rx))
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

/// Initialize a bare git repo at git_dir with mdd as work tree, if not already initialized.
fn git_init(mdd: &Path, git_dir: &Path) -> Result<(), String> {
    // If git_dir already contains a repo (has HEAD), skip
    if git_dir.join("HEAD").is_file() {
        return Ok(());
    }

    // Create git_dir if it doesn't exist
    std::fs::create_dir_all(git_dir)
        .map_err(|e| format!("failed to create git dir '{}': {}", git_dir.display(), e))?;

    // Init bare repo
    let output = Command::new("git")
        .args(["init", "--bare"])
        .arg(git_dir)
        .output()
        .map_err(|e| format!("git init --bare: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git init --bare failed: {}", stderr));
    }

    // Configure user for commits
    run_git(git_dir, mdd, &["config", "user.email", "pifs@localhost"])?;
    run_git(git_dir, mdd, &["config", "user.name", "pifs"])?;

    // Initial commit (allow empty)
    run_git(git_dir, mdd, &["commit", "--allow-empty", "-m", "pifs: initial commit"])?;
    Ok(())
}

/// Run a git command with --git-dir and --work-tree. Returns stdout on success.
fn run_git(git_dir: &Path, work_tree: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("--git-dir").arg(git_dir)
        .arg("--work-tree").arg(work_tree)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {}", args.join(" "), e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {} failed: {}", args.join(" "), stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Background loop: accumulate events, debounce 2s, then commit.
fn background_loop(mdd: PathBuf, git_dir: PathBuf, rx: mpsc::Receiver<GitEvent>) {
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
                    commit_pending(&mdd, &git_dir, &pending);
                    pending.clear();
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    commit_pending(&mdd, &git_dir, &pending);
                    break;
                }
            }
        };
        match event {
            GitEvent::Shutdown => {
                commit_pending(&mdd, &git_dir, &pending);
                break;
            }
            other => pending.push(other),
        }
    }
}

/// Stage all changes and commit with a descriptive message.
fn commit_pending(mdd: &Path, git_dir: &Path, events: &[GitEvent]) {
    if events.is_empty() {
        return;
    }

    // Stage all changes
    if run_git(git_dir, mdd, &["add", "-A"]).is_err() {
        return;
    }

    // Check if there's anything to commit
    if run_git(git_dir, mdd, &["diff", "--cached", "--quiet"]).is_ok() {
        return; // nothing staged
    }

    let msg = build_commit_message(events);
    let _ = run_git(git_dir, mdd, &["commit", "-m", &msg]);
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

    fn temp_dirs() -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("pifs-git-test-{}", std::process::id()));
        let base = base.join(format!("{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()));
        let mdd = base.join("mdd");
        let git = base.join("git");
        fs::create_dir_all(&mdd).unwrap();
        // git_init will create the git dir
        (mdd, git)
    }

    fn git_log_count(git_dir: &Path, mdd: &Path) -> usize {
        let out = run_git(git_dir, mdd, &["rev-list", "--count", "HEAD"]).unwrap();
        out.trim().parse().unwrap()
    }

    fn git_log_last_message(git_dir: &Path, mdd: &Path) -> String {
        run_git(git_dir, mdd, &["log", "-1", "--format=%B"]).unwrap()
    }

    #[test]
    fn test_git_init_creates_repo() {
        let (mdd, git) = temp_dirs();
        git_init(&mdd, &git).unwrap();
        assert!(git.join("HEAD").is_file());
        assert!(!mdd.join(".git").exists());
        assert_eq!(git_log_count(&git, &mdd), 1);
        fs::remove_dir_all(mdd.parent().unwrap()).unwrap();
    }

    #[test]
    fn test_git_init_skips_existing_repo() {
        let (mdd, git) = temp_dirs();
        git_init(&mdd, &git).unwrap();
        git_init(&mdd, &git).unwrap();
        assert_eq!(git_log_count(&git, &mdd), 1);
        fs::remove_dir_all(mdd.parent().unwrap()).unwrap();
    }

    #[test]
    fn test_single_file_change_commits() {
        let (mdd, git) = temp_dirs();
        let mut gs = GitSync::new(mdd.clone(), git.clone()).unwrap();

        fs::write(mdd.join("test.txt"), "hello\n").unwrap();
        gs.send(GitEvent::FileChanged(PathBuf::from("test.txt")));
        gs.shutdown();

        assert_eq!(git_log_count(&git, &mdd), 2);
        fs::remove_dir_all(mdd.parent().unwrap()).unwrap();
    }

    #[test]
    fn test_debounce_batches_events() {
        let (mdd, git) = temp_dirs();
        let mut gs = GitSync::new(mdd.clone(), git.clone()).unwrap();

        for i in 0..5 {
            let name = format!("file{}.txt", i);
            fs::write(mdd.join(&name), format!("content {}\n", i)).unwrap();
            gs.send(GitEvent::FileChanged(PathBuf::from(&name)));
        }

        gs.shutdown();

        assert_eq!(git_log_count(&git, &mdd), 2);
        fs::remove_dir_all(mdd.parent().unwrap()).unwrap();
    }

    #[test]
    fn test_shutdown_flushes_pending() {
        let (mdd, git) = temp_dirs();
        let mut gs = GitSync::new(mdd.clone(), git.clone()).unwrap();

        fs::write(mdd.join("pending.txt"), "data\n").unwrap();
        gs.send(GitEvent::FileChanged(PathBuf::from("pending.txt")));
        gs.shutdown();

        assert_eq!(git_log_count(&git, &mdd), 2);
        fs::remove_dir_all(mdd.parent().unwrap()).unwrap();
    }

    #[test]
    fn test_no_commit_when_nothing_changed() {
        let (mdd, git) = temp_dirs();
        let mut gs = GitSync::new(mdd.clone(), git.clone()).unwrap();

        gs.send(GitEvent::FileChanged(PathBuf::from("phantom.txt")));
        gs.shutdown();

        assert_eq!(git_log_count(&git, &mdd), 1);
        fs::remove_dir_all(mdd.parent().unwrap()).unwrap();
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
        let (mdd, git) = temp_dirs();
        let mut gs = GitSync::new(mdd.clone(), git.clone()).unwrap();

        let f = mdd.join("to_delete.txt");
        fs::write(&f, "bye\n").unwrap();
        gs.send(GitEvent::FileChanged(PathBuf::from("to_delete.txt")));
        gs.shutdown();

        let mut gs2 = GitSync::new(mdd.clone(), git.clone()).unwrap();
        fs::remove_file(&f).unwrap();
        gs2.send(GitEvent::FileDeleted(PathBuf::from("to_delete.txt")));
        gs2.shutdown();

        let msg = git_log_last_message(&git, &mdd);
        assert!(msg.contains("deleted: to_delete.txt"));
        fs::remove_dir_all(mdd.parent().unwrap()).unwrap();
    }

    #[test]
    fn test_drop_calls_shutdown() {
        let (mdd, git) = temp_dirs();

        {
            let gs = GitSync::new(mdd.clone(), git.clone()).unwrap();
            fs::write(mdd.join("drop_test.txt"), "drop\n").unwrap();
            gs.send(GitEvent::FileChanged(PathBuf::from("drop_test.txt")));
        }

        std::thread::sleep(Duration::from_millis(100));

        assert_eq!(git_log_count(&git, &mdd), 2);
        fs::remove_dir_all(mdd.parent().unwrap()).unwrap();
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
