use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiffError {
    #[error("I/O error")]
    Io(#[from] std::io::Error),
    #[error("image error")]
    Image(#[from] image::ImageError),
    #[error("zip error")]
    Zip(#[from] zip::result::ZipError),
    #[error("JSON error")]
    Json(#[from] serde_json::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, DiffError>;
