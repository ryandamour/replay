use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("git error: {0}")]
    Git(String),

    #[error(".replay/ directory not found (walked up from {0})")]
    NotInitialized(PathBuf),

    #[error("failed to acquire lock: {0}")]
    Lock(String),
}

pub type Result<T> = std::result::Result<T, ReplayError>;
