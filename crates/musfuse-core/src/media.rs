use async_trait::async_trait;
use bytes::Bytes;

use crate::error::Result;
use crate::metadata::TrackId;
use crate::policy::AudioFormatPolicy;
use crate::track::SourceTrack;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    pub data: Bytes,
    pub timestamp_ms: u64,
    pub is_end: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MediaContent {
    Stream(AudioChunk),
    Complete(Bytes),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscodeRequest {
    pub track: SourceTrack,
    pub policy: AudioFormatPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscodeResult {
    pub track_id: TrackId,
    pub format: &'static str,
    pub chunks: Vec<AudioChunk>,
}

#[async_trait]
pub trait AudioReader: Send + Sync {
    async fn read(&self, track: &SourceTrack) -> Result<Vec<AudioChunk>>;
}

#[async_trait]
pub trait FormatTranscoder: Send + Sync {
    async fn transcode(&self, request: &TranscodeRequest) -> Result<TranscodeResult>;
}

#[async_trait]
pub trait CoverExtractor: Send + Sync {
    async fn extract(&self, track: &SourceTrack) -> Result<Option<Vec<u8>>>;
}
