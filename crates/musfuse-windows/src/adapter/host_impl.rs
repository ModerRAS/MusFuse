use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use tracing::{debug, error, info};
use winfsp::host::{FileSystemHost, FileSystemParams, VolumeParams};
use winfsp::{winfsp_init, FspInit};

use musfuse_core::prelude::*;

use super::passthrough::PassthroughFS;
use super::winfsp::{WinFspHost, WinFspMountHandle};

/// Handle to keep the filesystem mounted
struct MountedHost {
    host: FileSystemHost<PassthroughFS>,
}

impl Drop for MountedHost {
    fn drop(&mut self) {
        info!("stopping filesystem dispatcher");
        self.host.stop();
        self.host.unmount();
    }
}

/// Implementation of WinFspHost that manages the filesystem lifecycle
pub struct WinFspHostImpl {
    _init: FspInit,
    mounted: Arc<Mutex<Option<MountedHost>>>,
}

impl WinFspHostImpl {
    /// Create a new WinFspHostImpl
    pub fn new() -> Result<Self> {
        let init = winfsp_init().map_err(|e| {
            MusFuseError::Mount(format!("failed to initialize WinFSP: {:?}", e))
        })?;

        Ok(Self {
            _init: init,
            mounted: Arc::new(Mutex::new(None)),
        })
    }
}

impl Default for WinFspHostImpl {
    fn default() -> Self {
        Self::new().expect("failed to initialize WinFSP")
    }
}

#[async_trait]
impl WinFspHost for WinFspHostImpl {
    async fn ensure_installed(&self) -> Result<()> {
        // WinFSP is already initialized in new(), so this is a no-op
        info!("WinFSP is installed and initialized");
        Ok(())
    }

    async fn mount(&self, config: &MountConfig) -> Result<WinFspMountHandle> {
        // Validate configuration
        config.validate()?;

        // Get source directory (we'll use the first one for M0)
        let source = config
            .sources
            .first()
            .ok_or_else(|| MusFuseError::Mount("no source directory configured".into()))?;

        let source_path = source.path.clone();
        debug!("mounting source: {:?} to {:?}", source_path, config.mount_point);

        // Create passthrough filesystem
        let fs = PassthroughFS::new(source_path.clone()).map_err(|e| {
            MusFuseError::Mount(format!("failed to create passthrough filesystem: {:?}", e))
        })?;

        // Configure volume parameters
        let mut volume_params = VolumeParams::new();
        volume_params
            .filesystem_name("MusFuse")
            .prefix("")
            .case_sensitive_search(false)
            .case_preserved_names(true)
            .unicode_on_disk(true)
            .persistent_acls(false)
            .reparse_points(false)
            .named_streams(false)
            .read_only_volume(false)
            .post_cleanup_when_modified_only(true)
            .pass_query_directory_pattern(true)
            .sector_size(4096)
            .sectors_per_allocation_unit(1)
            .max_component_length(255);

        // Create filesystem host
        let options = FileSystemParams::default_params(volume_params);
        let mut host = FileSystemHost::new_with_options(options, fs).map_err(|e| {
            MusFuseError::Mount(format!("failed to create filesystem host: {:?}", e))
        })?;

        // Start the dispatcher
        host.start().map_err(|e| {
            MusFuseError::Mount(format!("failed to start filesystem dispatcher: {:?}", e))
        })?;

        // Mount the filesystem
        let mount_point_str = config.mount_point.to_string_lossy();
        info!("mounting to: {}", mount_point_str);

        host.mount(mount_point_str.as_ref()).map_err(|e| {
            error!("failed to mount filesystem: {:?}", e);
            host.stop();
            MusFuseError::Mount(format!("failed to mount filesystem: {:?}", e))
        })?;

        info!("filesystem mounted successfully to {}", mount_point_str);

        // Store the host to keep it alive
        let mut mounted = self.mounted.lock();
        *mounted = Some(MountedHost { host });

        let mount_point = Arc::new(config.mount_point.clone());
        Ok(WinFspMountHandle { mount_point })
    }

    async fn unmount(&self, mount_point: &Path) -> Result<()> {
        info!("unmounting: {:?}", mount_point);
        
        let mut mounted = self.mounted.lock();
        if let Some(mut host) = mounted.take() {
            host.host.unmount();
            host.host.stop();
            info!("filesystem unmounted successfully");
        }
        
        Ok(())
    }
}
