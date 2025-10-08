use async_trait::async_trait;
use bytes::Bytes;
use flac_codec::encode::{FlacSampleWriter, Options};
use std::fs::File;
use std::io::Cursor;
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
        let data = task::spawn_blocking(move || std::fs::read(track_clone.path))
            .await
            .map_err(|err| MusFuseError::Media(err.to_string()))?
            .map_err(|err| MusFuseError::Media(err.to_string()))?;

        Ok(TranscodeResult {
            track_id: track.id.clone(),
            format,
            chunks: vec![AudioChunk {
                data: Bytes::from(data),
                timestamp_ms: 0,
                is_end: true,
            }],
        })
    }

    async fn convert_lossless(&self, track: &SourceTrack) -> Result<TranscodeResult> {
        let track_clone = track.clone();
        let flac = task::spawn_blocking(move || Self::encode_track_to_flac(&track_clone))
            .await
            .map_err(|err| MusFuseError::Media(err.to_string()))?
            .map_err(|err| MusFuseError::Media(err.to_string()))?;

        Ok(TranscodeResult {
            track_id: track.id.clone(),
            format: "flac",
            chunks: vec![AudioChunk {
                data: Bytes::from(flac),
                timestamp_ms: 0,
                is_end: true,
            }],
        })
    }

    fn encode_track_to_flac(track: &SourceTrack) -> Result<Vec<u8>> {
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
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
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
                    break
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

    fn encode_flac(decoded: DecodedAudio) -> Result<Vec<u8>> {
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

        Ok(cursor.into_inner())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{AlbumId, TrackId};
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
}
