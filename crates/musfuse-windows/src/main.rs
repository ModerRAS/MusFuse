use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use musfuse_core::prelude::*;
use musfuse_windows::{WindowsMountProvider, WinFspHostImpl};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(name = "musfuse")]
#[command(about = "MusFuse - Music Filesystem in Userspace", long_about = None)]
struct Args {
    /// Source directory to mount from
    #[arg(short, long)]
    source: PathBuf,

    /// Mount point (drive letter like M: or directory path)
    #[arg(short, long)]
    mount: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .init();

    info!("MusFuse starting...");
    info!("Source: {:?}", args.source);
    info!("Mount point: {:?}", args.mount);

    // Validate source directory
    if !args.source.exists() {
        error!("Source directory does not exist: {:?}", args.source);
        return Err(anyhow::anyhow!("Source directory does not exist"));
    }

    if !args.source.is_dir() {
        error!("Source path is not a directory: {:?}", args.source);
        return Err(anyhow::anyhow!("Source path is not a directory"));
    }

    // Create mount configuration
    let config = MountConfig {
        sources: vec![SourceConfig {
            path: args.source.clone(),
            recursive: true,
            watch: false,
        }],
        mount_point: args.mount.clone(),
        cache_dir: None,
        kv_backend: KvBackendKind::Sled,
        policies: PolicyConfig {
            lossless_strategy: LosslessStrategy::Passthrough,
            lossy_passthrough: true,
        },
        scan_mode: ScanMode::Lazy,
    };

    // Validate configuration
    config.validate()?;

    // Create WinFSP host
    let host = Arc::new(WinFspHostImpl::new()?);
    
    // Create mount provider
    let provider = WindowsMountProvider::with_winfsp_host(host);

    // Create mount context
    let context = Arc::new(MountContext::new(config));
    let mut event_rx = context.signal.subscribe();

    // Mount filesystem
    info!("Mounting filesystem...");
    provider.mount(context.clone()).await?;

    info!("Filesystem mounted successfully!");
    info!("Press Ctrl+C to unmount and exit...");

    // Wait for Ctrl+C
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, unmounting...");
        }
        event = event_rx.recv() => {
            match event {
                Ok(MountEvent::Fault(reason)) => {
                    error!("Filesystem fault: {}", reason);
                }
                Ok(MountEvent::Unmounted) => {
                    info!("Filesystem unmounted");
                }
                _ => {}
            }
        }
    }

    // Unmount filesystem
    provider.unmount().await?;
    info!("Filesystem unmounted successfully");

    Ok(())
}
