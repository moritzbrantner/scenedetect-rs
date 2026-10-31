use thiserror::Error;

pub type Result<T> = std::result::Result<T, SceneDetectError>;

#[derive(Debug, Error)]
pub enum SceneDetectError {
    #[error("invalid timecode: {0}")]
    InvalidTimecode(String),
    #[error("boundary review is not supported for {0}")]
    UnsupportedBoundaryReview(String),
    #[error("frame source error: {0}")]
    FrameSource(String),
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
