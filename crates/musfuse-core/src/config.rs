use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    pub sources: Vec<SourceConfig>,
    pub mount_point: PathBuf,
    pub cache_dir: Option<PathBuf>,
    pub kv_backend: KvBackendKind,
    pub policies: PolicyConfig,
    pub scan_mode: ScanMode,
}

impl MountConfig {
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.sources.is_empty() {
            return Err(ConfigValidationError::EmptySources);
        }
        if self.mount_point.as_os_str().is_empty() {
            return Err(ConfigValidationError::InvalidMountPoint);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceConfig {
    pub path: PathBuf,
    pub recursive: bool,
    pub watch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KvBackendKind {
    Sled,
    RocksDb,
    Sqlite,
    Redis,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScanMode {
    Eager,
    Lazy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyConfig {
    pub lossless_strategy: LosslessStrategy,
    pub lossy_passthrough: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LosslessStrategy {
    Passthrough,
    ConvertToFlac,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigValidationError {
    #[error("no source directories configured")]
    EmptySources,
    #[error("mount point must be provided")]
    InvalidMountPoint,
}
