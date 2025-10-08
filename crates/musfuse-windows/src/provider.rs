use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use musfuse_core::prelude::*;

pub struct WindowsMountProvider<A: PlatformAdapter> {
    adapter: Arc<A>,
    status: RwLock<MountStatus>,
    context: RwLock<Option<Arc<MountContext>>>,
}

impl<A: PlatformAdapter> WindowsMountProvider<A> {
    pub fn new(adapter: Arc<A>) -> Self {
        Self {
            adapter,
            status: RwLock::new(MountStatus::Unmounted),
            context: RwLock::new(None),
        }
    }

    fn transition_to_mounting(&self) -> Result<()> {
        let mut status = self.status.write();
        match &*status {
            MountStatus::Unmounted | MountStatus::Faulted(_) => {
                *status = MountStatus::Mounting;
                Ok(())
            }
            MountStatus::Mounting => Err(MusFuseError::Mount("mount already in progress".into())),
            MountStatus::Mounted => Err(MusFuseError::Mount("already mounted".into())),
            MountStatus::Unmounting => Err(MusFuseError::Mount("unmount currently in progress".into())),
        }
    }

    fn transition_to_unmounting(&self) -> Result<()> {
        let mut status = self.status.write();
        match &*status {
            MountStatus::Mounted => {
                *status = MountStatus::Unmounting;
                Ok(())
            }
            MountStatus::Unmounted => Ok(()),
            MountStatus::Mounting => Err(MusFuseError::Mount("cannot unmount while mounting".into())),
            MountStatus::Unmounting => Err(MusFuseError::Mount("unmount already in progress".into())),
            MountStatus::Faulted(_) => {
                *status = MountStatus::Unmounting;
                Ok(())
            }
        }
    }

    fn set_status(&self, status: MountStatus) {
        *self.status.write() = status;
    }

    fn update_context(&self, ctx: Option<Arc<MountContext>>) {
        *self.context.write() = ctx;
    }

    fn current_context(&self) -> Option<Arc<MountContext>> {
        self.context.read().clone()
    }

    fn emit_event(ctx: &MountContext, event: MountEvent) {
        let _ = ctx.signal.send(event);
    }

    fn handle_fault(&self, ctx: &Arc<MountContext>, err: MusFuseError) -> MusFuseError {
        let reason = err.to_string();
        self.set_status(MountStatus::Faulted(reason.clone()));
        Self::emit_event(ctx, MountEvent::Fault(reason));
        err
    }
}

#[async_trait]
impl<A: PlatformAdapter> MountProvider for WindowsMountProvider<A> {
    async fn mount(&self, ctx: Arc<MountContext>) -> Result<()> {
        self.transition_to_mounting()?;

        if let Err(err) = self.adapter.prepare_environment(&ctx.config).await {
            return Err(self.handle_fault(&ctx, err));
        }

        if let Err(err) = self.adapter.mount(&ctx.config).await {
            return Err(self.handle_fault(&ctx, err));
        }

        self.update_context(Some(ctx.clone()));
        self.set_status(MountStatus::Mounted);
        Self::emit_event(&ctx, MountEvent::Mounted);
        Ok(())
    }

    async fn unmount(&self) -> Result<()> {
        let ctx = match self.current_context() {
            Some(ctx) => ctx,
            None => {
                self.set_status(MountStatus::Unmounted);
                return Ok(());
            }
        };

        self.transition_to_unmounting()?;

        let mount_point = ctx.mount_point().to_path_buf();
        if let Err(err) = self.adapter.unmount(&mount_point).await {
            return Err(self.handle_fault(&ctx, err));
        }

        self.update_context(None);
        self.set_status(MountStatus::Unmounted);
        Self::emit_event(&ctx, MountEvent::Unmounted);
        Ok(())
    }

    fn status(&self) -> MountStatus {
        self.status.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use mockall::{mock, predicate::always};

    use musfuse_core::config::{LosslessStrategy, PolicyConfig, ScanMode, SourceConfig};

    mock! {
        pub Adapter {}

        #[async_trait]
        impl PlatformAdapter for Adapter {
            async fn prepare_environment(&self, config: &MountConfig) -> Result<()>;
            async fn mount(&self, config: &MountConfig) -> Result<()>;
            async fn unmount(&self, mount_point: &Path) -> Result<()>;
        }
    }

    fn sample_config() -> MountConfig {
        MountConfig {
            sources: vec![SourceConfig {
                path: "C:/Music".into(),
                recursive: true,
                watch: true,
            }],
            mount_point: "M:".into(),
            cache_dir: Some("C:/MusFuse/cache".into()),
            kv_backend: KvBackendKind::Sled,
            policies: PolicyConfig {
                lossless_strategy: LosslessStrategy::ConvertToFlac,
                lossy_passthrough: true,
            },
            scan_mode: ScanMode::Lazy,
        }
    }

    #[tokio::test]
    async fn mount_invokes_adapter_and_updates_status() {
        let mut mock_adapter = MockAdapter::new();
        mock_adapter
            .expect_prepare_environment()
            .with(always())
            .returning(|_| Ok(()));
        mock_adapter
            .expect_mount()
            .with(always())
            .returning(|_| Ok(()));

        let provider = WindowsMountProvider::new(Arc::new(mock_adapter));
        let ctx = Arc::new(MountContext::new(sample_config()));
        let mut rx = ctx.signal.subscribe();

        provider.mount(ctx.clone()).await.expect("mount should succeed");
        assert_eq!(provider.status(), MountStatus::Mounted);

        let event = rx.recv().await.expect("event expected");
        assert_eq!(event, MountEvent::Mounted);
    }

    #[tokio::test]
    async fn unmount_invokes_adapter_and_resets_status() {
        let mut mock_adapter = MockAdapter::new();
        mock_adapter
            .expect_prepare_environment()
            .returning(|_| Ok(()));
        mock_adapter.expect_mount().returning(|_| Ok(()));
        mock_adapter
            .expect_unmount()
            .withf(|path| path.to_string_lossy() == "M:")
            .returning(|_| Ok(()));

        let provider = WindowsMountProvider::new(Arc::new(mock_adapter));
        let ctx = Arc::new(MountContext::new(sample_config()));
        let mut rx = ctx.signal.subscribe();

        provider.mount(ctx.clone()).await.unwrap();
        let _ = rx.recv().await.unwrap(); // Mounted

        provider.unmount().await.expect("unmount should succeed");
        assert_eq!(provider.status(), MountStatus::Unmounted);
        let event = rx.recv().await.expect("unmount event expected");
        assert_eq!(event, MountEvent::Unmounted);
    }

    #[tokio::test]
    async fn mount_failure_moves_to_fault_state() {
        let mut mock_adapter = MockAdapter::new();
        mock_adapter
            .expect_prepare_environment()
            .returning(|_| Ok(()));
        mock_adapter
            .expect_mount()
            .returning(|_| Err(MusFuseError::Mount("mount failed".into())));

        let provider = WindowsMountProvider::new(Arc::new(mock_adapter));
        let ctx = Arc::new(MountContext::new(sample_config()));
        let mut rx = ctx.signal.subscribe();

        let err = provider.mount(ctx.clone()).await.expect_err("should fail");
        assert!(matches!(err, MusFuseError::Mount(_)));

        match provider.status() {
            MountStatus::Faulted(reason) => assert!(reason.contains("mount failed")),
            other => panic!("unexpected status {other:?}", other = other),
        }

        let event = rx.recv().await.expect("fault event expected");
        match event {
            MountEvent::Fault(reason) => assert!(reason.contains("mount failed")),
            other => panic!("unexpected event {other:?}", other = other),
        }
    }
}
