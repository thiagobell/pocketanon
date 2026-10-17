use std::path::PathBuf;

/// Errors from loading Models, running inference and managing the model directory.
#[derive(Debug, thiserror::Error)]
pub enum GlinerError {
    #[error("model directory {0} is missing {1}")]
    MissingFile(PathBuf, String),
    #[error("model {model} has no {variant} variant")]
    UnknownVariant { model: String, variant: String },
    #[error("could not determine the platform data directory")]
    NoDataDir,
    #[error("failed to load model: {0}")]
    Load(String),
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("download of {url} failed: {message}")]
    Download { url: String, message: String },
    #[error("download of {file} incomplete: expected {expected} bytes, got {received}")]
    SizeMismatch { file: String, expected: u64, received: u64 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
