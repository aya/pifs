mod file_data;
mod filesystem;
mod git_sync;
mod ipfs;
mod types;

use std::path::PathBuf;

use clap::Parser;
use fuser::MountOption;

use file_data::StorageMode;
use git_sync::GitSync;
use types::PifsFilesystem;

const PIFS_VERSION: &str = "0.1.0";

#[derive(Parser)]
#[command(name = "pifs", version = PIFS_VERSION, about = "FUSE filesystem using IPFS as backend storage")]
struct Cli {
    /// Metadata directory to store IPFS hashes
    #[arg(long)]
    mdd: PathBuf,

    /// Log file to trace FUSE calls
    #[arg(long)]
    log: Option<PathBuf>,

    /// Use whole-file storage mode (legacy). Default is chunked (256KB chunks).
    #[arg(long)]
    whole_file: bool,

    /// Enable git versioning of the MDD. Takes the path to the git directory (stored separately from the MDD).
    #[arg(long)]
    git: Option<PathBuf>,

    /// Mount point
    mountpoint: PathBuf,
}

fn main() {
    let cli = Cli::parse();

    // Validate mdd directory
    if !cli.mdd.is_dir() {
        eprintln!(
            "pifs: Metadata directory '{}' does not exist or is not a directory",
            cli.mdd.display()
        );
        std::process::exit(1);
    }

    // Validate mountpoint
    if !cli.mountpoint.exists() {
        eprintln!(
            "pifs: Mount point '{}' does not exist",
            cli.mountpoint.display()
        );
        std::process::exit(1);
    }

    // Setup logging
    if let Some(ref log_path) = cli.log {
        // Create/truncate the log file to validate it's writable
        match std::fs::File::create(log_path) {
            Ok(_) => {}
            Err(e) => {
                eprintln!(
                    "pifs: Cannot write to log file '{}': {}",
                    log_path.display(),
                    e
                );
                std::process::exit(1);
            }
        }

        // Use RUST_LOG env var or default to info
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", "info");
        }
        env_logger::init();
        log::info!(
            "starting pifs v{}, mdd={}, mountpoint={}, log={:?}",
            PIFS_VERSION,
            cli.mdd.display(),
            cli.mountpoint.display(),
            cli.log
        );
    } else {
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", "warn");
        }
        env_logger::init();
    }

    let storage_mode = if cli.whole_file {
        StorageMode::WholeFile
    } else {
        StorageMode::Chunked
    };

    let mdd = cli.mdd.canonicalize().unwrap_or(cli.mdd);

    let git_sync = if let Some(ref git_dir) = cli.git {
        match GitSync::new(mdd.clone(), git_dir.clone()) {
            Ok(gs) => {
                log::info!("git MDD versioning enabled, git_dir={}", git_dir.display());
                Some(gs)
            }
            Err(e) => {
                eprintln!("pifs: Failed to initialize git: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    let fs = PifsFilesystem::new(
        mdd,
        cli.log.clone(),
        storage_mode,
        git_sync,
    );

    let options = vec![
        MountOption::FSName("pifs".to_string()),
        MountOption::AutoUnmount,
        MountOption::AllowOther,
        // macFUSE kills the mount after 2*daemon_timeout of inactivity (default 60s).
        // Set high to avoid spurious disconnects during idle periods.
        MountOption::CUSTOM("daemon_timeout=600".to_string()),
    ];

    log::info!("mounting pifs at {} (storage: {:?})", cli.mountpoint.display(), storage_mode);
    if let Err(e) = fuser::mount2(fs, &cli.mountpoint, &options) {
        eprintln!("pifs: Failed to mount: {}", e);
        std::process::exit(1);
    }

    log::info!("pifs unmounted");
}
