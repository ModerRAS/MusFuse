use serde::{Deserialize, Serialize};

use crate::config::{LosslessStrategy, PolicyConfig};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AudioFormatPolicy {
    PassthroughLossy,
    PassthroughLossless,
    ConvertLossless,
}

impl AudioFormatPolicy {
    pub fn from_extension(ext: &str, config: &PolicyConfig) -> Self {
        let lowered = ext.to_ascii_lowercase();
        match lowered.as_str() {
            "mp3" | "aac" | "ogg" | "opus" | "m4a" => AudioFormatPolicy::PassthroughLossy,
            _ => match config.lossless_strategy {
                LosslessStrategy::Passthrough => AudioFormatPolicy::PassthroughLossless,
                LosslessStrategy::ConvertToFlac => AudioFormatPolicy::ConvertLossless,
            },
        }
    }
}
