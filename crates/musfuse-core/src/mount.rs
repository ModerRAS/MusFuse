use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::config::MountConfig;
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MountStatus {
    Unmounted,
    Mounting,
    Mounted,
    Unmounting,
    Faulted(String),
}

#[async_trait]
pub trait MountProvider: Send + Sync {
    async fn mount(&self, ctx: Arc<MountContext>) -> Result<()>;
    async fn unmount(&self) -> Result<()>;
    fn status(&self) -> MountStatus;
}

#[derive(Debug)]
pub struct MountContext {
    pub config: Arc<MountConfig>,
    pub signal: broadcast::Sender<MountEvent>,
}

impl MountContext {
    pub fn new(config: MountConfig) -> Self {
        let (signal, _) = broadcast::channel(4);
        Self {
            config: Arc::new(config),
            signal,
        }
    }

    pub fn mount_point(&self) -> &Path {
        &self.config.mount_point
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MountEvent {
    Mounted,
    Unmounted,
    Fault(String),
}

#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    async fn prepare_environment(&self, config: &MountConfig) -> Result<()>;
    async fn mount(&self, config: &MountConfig) -> Result<()>;
    async fn unmount(&self, mount_point: &Path) -> Result<()>;
}
