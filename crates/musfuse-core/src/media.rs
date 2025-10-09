use async_trait::async_trait;
use bytes::Bytes;
use flac_codec::encode::{FlacSampleWriter, Options};
use lofty::{Picture, PictureType, TaggedFileExt, read_from_path};
use std::fs::{self, File};
use std::io::{Cursor, ErrorKind, Read};
use std::path::{Path, PathBuf};
use tokio::task;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::error::{MusFuseError, Result};
use crate::metadata::TrackId;
use crate::policy::AudioFormatPolicy;
use crate::track::SourceTrack;

const DEFAULT_CHUNK_SIZE: usize = 256 * 1024; // 256 KiB
const FALLBACK_CHUNK_DURATION_MS: u64 = 200;

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

pub struct DefaultFormatTranscoder;

pub struct DefaultCoverExtractor;

impl DefaultFormatTranscoder {
    pub fn new() -> Self {
        Self
    }

    fn extension_of(track: &SourceTrack) -> &'static str {
        track
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref()
            .map(|ext| match ext {
                "wav" => "wav",
                "flac" => "flac",
                "ogg" => "ogg",
                "mp3" => "mp3",
                "aac" => "aac",
                "m4a" => "m4a",
                _ => "bin",
            })
            .unwrap_or("bin")
    }

    async fn passthrough(&self, track: &SourceTrack) -> Result<TranscodeResult> {
        let format = Self::extension_of(track);
        let track_clone = track.clone();
        let sample_rate = track.sample_rate;
        let channels = track.channels;
        let chunks = task::spawn_blocking(move || {
            Self::passthrough_chunks(track_clone.path, sample_rate, channels)
        })
        .await
        .map_err(|err| MusFuseError::Media(err.to_string()))??;

        Ok(TranscodeResult {
            track_id: track.id.clone(),
            format,
            chunks,
        })
    }

    async fn convert_lossless(&self, track: &SourceTrack) -> Result<TranscodeResult> {
        let track_clone = track.clone();
        let encoded = task::spawn_blocking(move || Self::encode_track_to_flac(&track_clone))
            .await
            .map_err(|err| MusFuseError::Media(err.to_string()))?
            .map_err(|err| MusFuseError::Media(err.to_string()))?;

        let chunks = Self::chunk_bytes(
            encoded.data,
            DEFAULT_CHUNK_SIZE,
            Some(encoded.sample_rate),
            Some(encoded.channels),
            Some(encoded.bits_per_sample),
        );

        Ok(TranscodeResult {
            track_id: track.id.clone(),
            format: "flac",
            chunks,
        })
    }

    fn passthrough_chunks(
        path: PathBuf,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Vec<AudioChunk>> {
        let mut file = File::open(&path)?;
        let mut buffer = vec![0u8; DEFAULT_CHUNK_SIZE];
        let mut total_bytes: usize = 0;
        let mut index: usize = 0;
        let frame_bytes = Self::bytes_per_frame(Some(channels), None);
        let sample_rate_opt = if sample_rate > 0 {
            Some(sample_rate)
        } else {
            None
        };
        let mut chunks = Vec::new();

        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }

            let timestamp_ms =
                Self::offset_to_timestamp(total_bytes, frame_bytes, sample_rate_opt, index);
            let bytes = Bytes::copy_from_slice(&buffer[..read]);
            chunks.push(AudioChunk {
                data: bytes,
                timestamp_ms,
                is_end: false,
            });

            total_bytes += read;
            index += 1;
        }

        if let Some(last) = chunks.last_mut() {
            last.is_end = true;
        }

        Ok(chunks)
    }

    fn chunk_bytes(
        data: Vec<u8>,
        chunk_size: usize,
        sample_rate: Option<u32>,
        channels: Option<u16>,
        bits_per_sample: Option<u16>,
    ) -> Vec<AudioChunk> {
        if data.is_empty() {
            return vec![];
        }

        let frame_bytes = Self::bytes_per_frame(channels, bits_per_sample);
        let mut offset_bytes = 0usize;
        let mut index = 0usize;

        data.chunks(chunk_size)
            .map(|chunk| {
                let timestamp_ms =
                    Self::offset_to_timestamp(offset_bytes, frame_bytes, sample_rate, index);
                offset_bytes += chunk.len();
                index += 1;
                AudioChunk {
                    data: Bytes::copy_from_slice(chunk),
                    timestamp_ms,
                    is_end: false,
                }
            })
            .enumerate()
            .map(|(idx, mut chunk)| {
                let is_last = idx == (data.len() + chunk_size - 1) / chunk_size - 1;
                if is_last {
                    chunk.is_end = true;
                }
                chunk
            })
            .collect()
    }

    fn bytes_per_frame(channels: Option<u16>, bits_per_sample: Option<u16>) -> Option<usize> {
        let channels = channels.filter(|c| *c > 0)? as usize;
        let bits = bits_per_sample.unwrap_or(16).max(8) as usize;
        let bytes_per_sample = bits / 8;
        Some(channels * bytes_per_sample.max(1))
    }

    fn offset_to_timestamp(
        offset_bytes: usize,
        frame_bytes: Option<usize>,
        sample_rate: Option<u32>,
        chunk_index: usize,
    ) -> u64 {
        if let (Some(frame_bytes), Some(sample_rate)) = (frame_bytes, sample_rate) {
            if frame_bytes > 0 && sample_rate > 0 {
                let frames = offset_bytes / frame_bytes;
                return (frames as u64 * 1_000) / sample_rate as u64;
            }
        }

        chunk_index as u64 * FALLBACK_CHUNK_DURATION_MS
    }

    fn encode_track_to_flac(track: &SourceTrack) -> Result<EncodedAudio> {
        let decoded = Self::decode_track(track)?;
        Self::encode_flac(decoded)
    }

    fn decode_track(track: &SourceTrack) -> Result<DecodedAudio> {
        let file = File::open(&track.path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = track.path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|err| MusFuseError::Media(err.to_string()))?;

        let mut format = probed.format;
        let track_info = format
            .default_track()
            .ok_or_else(|| MusFuseError::Media("no default audio track".into()))?;

        let codec_params = &track_info.codec_params;
        let sample_rate = codec_params
            .sample_rate
            .ok_or_else(|| MusFuseError::Media("missing sample rate".into()))?;
        let channels = codec_params
            .channels
            .ok_or_else(|| MusFuseError::Media("missing channel layout".into()))?;

        let channel_count = channels.count() as u8;
        if channel_count == 0 {
            return Err(MusFuseError::Media("zero channel count".into()));
        }

        let bits_per_sample = codec_params.bits_per_sample.unwrap_or(16) as u32;

        let mut decoder = symphonia::default::get_codecs()
            .make(codec_params, &DecoderOptions::default())
            .map_err(|err| MusFuseError::Media(err.to_string()))?;

        let start_frame = track.offset_frames;
        let end_frame = if track.length_frames > 0 {
            start_frame + track.length_frames
        } else {
            u64::MAX
        };

        let mut current_frame: u64 = 0;
        let mut samples: Vec<i32> = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(err))
                    if err.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(SymphoniaError::ResetRequired) => {
                    decoder.reset();
                    continue;
                }
                Err(err) => return Err(MusFuseError::Media(err.to_string())),
            };

            let decoded = decoder
                .decode(&packet)
                .map_err(|err| MusFuseError::Media(err.to_string()))?;

            let spec = *decoded.spec();
            let mut sample_buf = SampleBuffer::<i32>::new(decoded.capacity() as u64, spec);
            sample_buf.copy_interleaved_ref(decoded);
            let buffer_samples = sample_buf.samples();
            if buffer_samples.is_empty() {
                current_frame += 0;
                continue;
            }

            let frame_count = (buffer_samples.len() / channel_count as usize) as u64;
            let buffer_start = current_frame;
            let buffer_end = current_frame + frame_count;

            let select_start = start_frame.max(buffer_start);
            let select_end = end_frame.min(buffer_end);

            if select_end > select_start {
                let start_idx = (select_start - buffer_start) as usize * channel_count as usize;
                let end_idx = (select_end - buffer_start) as usize * channel_count as usize;
                samples.extend_from_slice(&buffer_samples[start_idx..end_idx]);
            }

            current_frame = buffer_end;

            if current_frame >= end_frame {
                break;
            }
        }

        if samples.is_empty() {
            return Err(MusFuseError::Media("no audio samples decoded".into()));
        }

        Ok(DecodedAudio {
            samples,
            sample_rate,
            channels: channel_count,
            bits_per_sample,
        })
    }

    fn encode_flac(decoded: DecodedAudio) -> Result<EncodedAudio> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = FlacSampleWriter::new(
                &mut cursor,
                Options::default(),
                decoded.sample_rate,
                decoded.bits_per_sample,
                decoded.channels,
                None,
            )
            .map_err(|err| MusFuseError::Media(err.to_string()))?;

            writer
                .write(&decoded.samples)
                .map_err(|err| MusFuseError::Media(err.to_string()))?;
            writer
                .finalize()
                .map_err(|err| MusFuseError::Media(err.to_string()))?;
        }

        Ok(EncodedAudio {
            data: cursor.into_inner(),
            sample_rate: decoded.sample_rate,
            channels: decoded.channels as u16,
            bits_per_sample: decoded.bits_per_sample as u16,
        })
    }
}

impl DefaultCoverExtractor {
    pub fn new() -> Self {
        Self
    }

    fn extract_sync(path: PathBuf) -> Result<Option<Vec<u8>>> {
        if let Some(bytes) = Self::extract_embedded(&path)? {
            return Ok(Some(bytes));
        }
        Self::extract_external(&path)
    }

    fn extract_embedded(path: &Path) -> Result<Option<Vec<u8>>> {
        let tagged = match read_from_path(path) {
            Ok(tagged) => tagged,
            Err(_) => return Ok(None),
        };

        if let Some(primary) = tagged.primary_tag() {
            if let Some(bytes) = Self::select_picture(primary.pictures()) {
                return Ok(Some(bytes));
            }
        }

        if let Some(first) = tagged.first_tag() {
            if let Some(bytes) = Self::select_picture(first.pictures()) {
                return Ok(Some(bytes));
            }
        }

        for tag in tagged.tags() {
            if let Some(bytes) = Self::select_picture(tag.pictures()) {
                return Ok(Some(bytes));
            }
        }

        Ok(None)
    }

    fn extract_external(path: &Path) -> Result<Option<Vec<u8>>> {
        let dir = match path.parent() {
            Some(dir) => dir,
            None => return Ok(None),
        };

        for candidate in Self::candidate_paths(dir, path.file_stem()) {
            match fs::read(&candidate) {
                Ok(bytes) if !bytes.is_empty() => return Ok(Some(bytes)),
                Ok(_) => continue,
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(err) => return Err(MusFuseError::Io(err)),
            }
        }

        Ok(None)
    }

    fn candidate_paths(dir: &Path, stem: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
        const CANDIDATES: &[&str] = &[
            "cover.jpg",
            "cover.jpeg",
            "cover.png",
            "cover.webp",
            "folder.jpg",
            "folder.jpeg",
            "folder.png",
            "AlbumArtSmall.jpg",
        ];

        let mut paths: Vec<PathBuf> = CANDIDATES.iter().map(|name| dir.join(name)).collect();

        if let Some(stem) = stem.and_then(|s| s.to_str()) {
            for ext in &["jpg", "jpeg", "png", "webp"] {
                paths.push(dir.join(format!("{}.{ext}", stem)));
            }
        }

        paths
    }

    fn select_picture(pictures: &[Picture]) -> Option<Vec<u8>> {
        let mut front = None;
        let mut fallback = None;

        for picture in pictures {
            if picture.data().is_empty() {
                continue;
            }

            if picture.pic_type() == PictureType::CoverFront {
                front = Some(picture.data().to_vec());
                break;
            }

            if fallback.is_none() {
                fallback = Some(picture.data().to_vec());
            }
        }

        front.or(fallback)
    }
}

#[async_trait]
impl CoverExtractor for DefaultCoverExtractor {
    async fn extract(&self, track: &SourceTrack) -> Result<Option<Vec<u8>>> {
        let path = track.path.clone();
        task::spawn_blocking(move || Self::extract_sync(path))
            .await
            .map_err(|err| MusFuseError::Media(err.to_string()))?
    }
}

#[async_trait]
impl FormatTranscoder for DefaultFormatTranscoder {
    async fn transcode(&self, request: &TranscodeRequest) -> Result<TranscodeResult> {
        match request.policy {
            AudioFormatPolicy::PassthroughLossy | AudioFormatPolicy::PassthroughLossless => {
                self.passthrough(&request.track).await
            }
            AudioFormatPolicy::ConvertLossless => self.convert_lossless(&request.track).await,
        }
    }
}

struct DecodedAudio {
    samples: Vec<i32>,
    sample_rate: u32,
    channels: u8,
    bits_per_sample: u32,
}

struct EncodedAudio {
    data: Vec<u8>,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{AlbumId, TrackId};
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use tempfile::tempdir;

    fn write_test_wav(path: &Path) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("create wav");
        for _ in 0..1_000 {
            writer.write_sample(0i16).expect("write left");
            writer.write_sample(0i16).expect("write right");
        }
        writer.finalize().expect("finalize wav");
    }

    fn make_track(path: &Path) -> SourceTrack {
        SourceTrack {
            id: TrackId {
                album: AlbumId("album".into()),
                disc: 1,
                index: 1,
            },
            path: path.to_path_buf(),
            cue_path: None,
            offset_frames: 0,
            length_frames: 0,
            sample_rate: 44_100,
            channels: 2,
        }
    }

    #[tokio::test]
    async fn passthrough_lossless_returns_original_wav() {
        let dir = tempdir().expect("tempdir");
        let wav_path = dir.path().join("sample.wav");
        write_test_wav(&wav_path);

        let transcoder = DefaultFormatTranscoder::new();
        let request = TranscodeRequest {
            track: make_track(&wav_path),
            policy: AudioFormatPolicy::PassthroughLossless,
        };

        let result = transcoder.transcode(&request).await.expect("transcode");
        assert_eq!(result.track_id, request.track.id);
        assert_eq!(result.format, "wav");
        assert_eq!(result.chunks.len(), 1);
        let data = &result.chunks[0].data;
        assert!(data.starts_with(b"RIFF"));
        assert!(result.chunks[0].is_end);
    }

    #[tokio::test]
    async fn convert_lossless_outputs_flac() {
        let dir = tempdir().expect("tempdir");
        let wav_path = dir.path().join("sample.wav");
        write_test_wav(&wav_path);

        let transcoder = DefaultFormatTranscoder::new();
        let request = TranscodeRequest {
            track: make_track(&wav_path),
            policy: AudioFormatPolicy::ConvertLossless,
        };

        let result = transcoder.transcode(&request).await.expect("transcode");
        assert_eq!(result.track_id, request.track.id);
        assert_eq!(result.format, "flac");
        assert_eq!(result.chunks.len(), 1);
        let data = &result.chunks[0].data;
        assert!(data.starts_with(b"fLaC"));
        assert!(result.chunks[0].is_end);
    }

    #[test]
    fn chunk_bytes_splits_data_into_multiple_chunks() {
        let data = vec![1u8; DEFAULT_CHUNK_SIZE * 2 + 10];
        let chunks = DefaultFormatTranscoder::chunk_bytes(
            data,
            DEFAULT_CHUNK_SIZE,
            Some(44_100),
            Some(2),
            Some(16),
        );

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks.iter().filter(|chunk| chunk.is_end).count(), 1);
        assert!(
            chunks.first().map(|c| c.timestamp_ms).unwrap_or_default()
                <= chunks.last().map(|c| c.timestamp_ms).unwrap_or_default()
        );
    }

    #[tokio::test]
    async fn cover_extractor_reads_external_cover() {
        let dir = tempdir().expect("tempdir");
        let wav_path = dir.path().join("track.wav");
        write_test_wav(&wav_path);

        let cover_path = dir.path().join("cover.jpg");
        let mut cover_file = fs::File::create(&cover_path).expect("cover file");
        cover_file.write_all(&[1u8, 2, 3, 4]).expect("write cover");
        cover_file.flush().expect("flush cover");

        let extractor = DefaultCoverExtractor::new();
        let track = make_track(&wav_path);
        let result = extractor.extract(&track).await.expect("extract");

        assert_eq!(result, Some(vec![1u8, 2, 3, 4]));
    }
}
