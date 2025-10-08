use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MusFuseError {
    #[error("configuration error: {0}")]
    Config(#[from] crate::config::ConfigValidationError),
    #[error("kv backend error: {0}")]
    Kv(String),
    #[error("mount error: {0}")]
    Mount(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("unsupported operation: {0}")]
    Unsupported(&'static str),
}

pub type Result<T, E = MusFuseError> = std::result::Result<T, E>;
