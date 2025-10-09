use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use musfuse_core::prelude::*;

#[async_trait]
pub trait WinFspHost: Send + Sync {
    async fn ensure_installed(&self) -> Result<()>;
    async fn mount(&self, config: &MountConfig) -> Result<WinFspMountHandle>;
    async fn unmount(&self, mount_point: &Path) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct WinFspMountHandle {
    pub mount_point: Arc<PathBuf>,
}

pub struct WinFspAdapter<H: WinFspHost> {
    host: Arc<H>,
}

impl<H: WinFspHost> WinFspAdapter<H> {
    pub fn new(host: Arc<H>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl<H: WinFspHost> PlatformAdapter for WinFspAdapter<H> {
    async fn prepare_environment(&self, config: &MountConfig) -> Result<()> {
        if config.mount_point.as_os_str().is_empty() {
            return Err(MusFuseError::Mount("missing mount point".into()));
        }
        self.host.ensure_installed().await
    }

    async fn mount(&self, config: &MountConfig) -> Result<()> {
        self.host.mount(config).await.map(|_| ())
    }

    async fn unmount(&self, mount_point: &Path) -> Result<()> {
        self.host.unmount(mount_point).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::{mock, predicate::always};

    mock! {
        pub Host {}

        #[async_trait]
        impl WinFspHost for Host {
            async fn ensure_installed(&self) -> Result<()>;
            async fn mount(&self, config: &MountConfig) -> Result<WinFspMountHandle>;
            async fn unmount(&self, mount_point: &Path) -> Result<()>;
        }
    }

    fn sample_config() -> MountConfig {
        MountConfig {
            sources: vec![],
            mount_point: PathBuf::from("M:"),
            cache_dir: None,
            kv_backend: KvBackendKind::Sled,
            policies: PolicyConfig {
                lossless_strategy: LosslessStrategy::ConvertToFlac,
                lossy_passthrough: true,
            },
            scan_mode: ScanMode::Lazy,
        }
    }

    #[tokio::test]
    async fn prepare_environment_calls_host() {
        let mut mock_host = MockHost::new();
        mock_host.expect_ensure_installed().return_once(|| Ok(()));
        let adapter = WinFspAdapter::new(Arc::new(mock_host));
        adapter
            .prepare_environment(&sample_config())
            .await
            .expect("prepare should succeed");
    }

    #[tokio::test]
    async fn prepare_environment_fails_when_mount_point_missing() {
        let mock_host = MockHost::new();
        let adapter = WinFspAdapter::new(Arc::new(mock_host));
        let mut config = sample_config();
        config.mount_point = PathBuf::new();
        let err = adapter
            .prepare_environment(&config)
            .await
            .expect_err("should fail");
        assert!(matches!(err, MusFuseError::Mount(_)));
    }

    #[tokio::test]
    async fn mount_calls_host_and_discards_handle() {
        let mut mock_host = MockHost::new();
        mock_host.expect_mount().with(always()).returning(|_| {
            Ok(WinFspMountHandle {
                mount_point: Arc::new(PathBuf::from("M:")),
            })
        });
        let adapter = WinFspAdapter::new(Arc::new(mock_host));
        adapter
            .mount(&sample_config())
            .await
            .expect("mount should succeed");
    }

    #[tokio::test]
    async fn unmount_calls_host() {
        let mut mock_host = MockHost::new();
        mock_host
            .expect_unmount()
            .withf(|p| p.to_string_lossy() == "M:")
            .return_once(|_| Ok(()));
        let adapter = WinFspAdapter::new(Arc::new(mock_host));
        adapter
            .unmount(Path::new("M:"))
            .await
            .expect("unmount should succeed");
    }
}
