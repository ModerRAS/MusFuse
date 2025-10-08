use std::path::PathBuf;
use std::time::SystemTime;

use async_trait::async_trait;

use crate::config::ScanMode;
use crate::error::Result;
use crate::metadata::{AlbumId, TrackId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanRecord {
    pub source: PathBuf,
    pub modified: SystemTime,
    pub tracks: Vec<TrackId>,
    pub albums: Vec<AlbumId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanEvent {
    FileAdded(PathBuf),
    FileRemoved(PathBuf),
    FileModified(PathBuf),
    AlbumUpdated(AlbumId),
}

#[async_trait]
pub trait LibraryScanner: Send + Sync {
    async fn full_scan(&self, mode: ScanMode) -> Result<Vec<ScanRecord>>;
    async fn refresh_paths(&self, paths: &[PathBuf]) -> Result<Vec<ScanEvent>>;
    async fn watch(&self) -> Result<()>;
}
