mod filesystem;
mod ipfs;
mod types;

use std::path::PathBuf;

use clap::Parser;
use fuser::MountOption;

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

    let fs = PifsFilesystem::new(
        cli.mdd.canonicalize().unwrap_or(cli.mdd),
        cli.log.clone(),
    );

    let options = vec![
        MountOption::FSName("pifs".to_string()),
        MountOption::AutoUnmount,
        MountOption::AllowOther,
        // macFUSE kills the mount after 2*daemon_timeout of inactivity (default 60s).
        // Set high to avoid spurious disconnects during idle periods.
        MountOption::CUSTOM("daemon_timeout=600".to_string()),
    ];

    log::info!("mounting pifs at {}", cli.mountpoint.display());
    if let Err(e) = fuser::mount2(fs, &cli.mountpoint, &options) {
        eprintln!("pifs: Failed to mount: {}", e);
        std::process::exit(1);
    }

    log::info!("pifs unmounted");
}
