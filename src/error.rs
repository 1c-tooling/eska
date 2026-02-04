use thiserror::Error;

#[derive(Debug, Error)]
pub enum EskaError {
    #[error("Error [IO]: {0}")]
    Io(#[from] std::io::Error),

    #[error("Error [Eska]: {0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, EskaError>;
