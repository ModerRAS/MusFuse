use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::config::PolicyConfig;
use crate::media::{AudioReader, CoverExtractor, FormatTranscoder, TranscodeRequest};
use crate::metadata::{TagDelta, TrackId, TrackMetadata};
use crate::track::TrackIndexEntry;
use crate::tag::TagOverlayService;

#[derive(Debug, Clone, PartialEq)]
pub enum VirtualEntry {
    Directory(PathBuf),
    TrackFile(TrackId),
    CoverImage(TrackId),
}

#[allow(dead_code)]
pub struct MediaEngine {
    reader: Arc<dyn AudioReader>,
    transcoder: Arc<dyn FormatTranscoder>,
    cover: Arc<dyn CoverExtractor>,
    policy: PolicyConfig,
}

impl MediaEngine {
    pub fn new(
        reader: Arc<dyn AudioReader>,
        transcoder: Arc<dyn FormatTranscoder>,
        cover: Arc<dyn CoverExtractor>,
        policy: PolicyConfig,
    ) -> Self {
        Self {
            reader,
            transcoder,
            cover,
            policy,
        }
    }

    pub async fn stream_track(&self, entry: &TrackIndexEntry) -> Result<Vec<u8>> {
        let policy = crate::policy::AudioFormatPolicy::from_extension("flac", &self.policy);
        let request = TranscodeRequest {
            track: entry.source.clone(),
            policy,
        };
        let result = self.transcoder.transcode(&request).await?;
        let mut buffer = Vec::new();
        for chunk in result.chunks {
            buffer.extend_from_slice(&chunk.data);
        }
        Ok(buffer)
    }

    pub async fn cover_image(&self, entry: &TrackIndexEntry) -> Result<Option<Vec<u8>>> {
        self.cover.extract(&entry.source).await
    }
}

pub struct FileRouter {
    index: Arc<Vec<TrackIndexEntry>>,
    media: Arc<MediaEngine>,
    tags: Arc<dyn TagOverlayService>,
}

impl FileRouter {
    pub fn new(
        index: Arc<Vec<TrackIndexEntry>>,
        media: Arc<MediaEngine>,
        tags: Arc<dyn TagOverlayService>,
    ) -> Self {
        Self { index, media, tags }
    }

    pub fn resolve(&self, path: &str) -> Option<VirtualEntry> {
        let path = path.trim_matches('/');
        if path.is_empty() {
            return Some(VirtualEntry::Directory(PathBuf::from("/")));
        }

        let candidate = path.strip_suffix(".flac").unwrap_or(path);

        self.index
            .iter()
            .find(|entry| entry.id.to_string() == candidate)
            .map(|entry| VirtualEntry::TrackFile(entry.id.clone()))
    }

    pub async fn read_track(&self, id: &TrackId) -> Result<Vec<u8>> {
        let entry = self
            .index
            .iter()
            .find(|entry| &entry.id == id)
            .ok_or_else(|| crate::error::MusFuseError::Mount("track not found".into()))?;
        self.media.stream_track(entry).await
    }

    pub async fn read_tags(&self, id: &TrackId) -> Result<TrackMetadata> {
        let entry = self
            .index
            .iter()
            .find(|entry| &entry.id == id)
            .ok_or_else(|| crate::error::MusFuseError::Mount("track not found".into()))?;
        self.tags.read(id, &entry.source.path).await
    }

    pub async fn write_tags(&self, id: &TrackId, delta: &TagDelta) -> Result<TrackMetadata> {
        let entry = self
            .index
            .iter()
            .find(|entry| &entry.id == id)
            .ok_or_else(|| crate::error::MusFuseError::Mount("track not found".into()))?;
        self.tags.apply(id, &entry.source.path, delta).await
    }
}
