use thiserror::Error;

#[derive(Debug, Error)]
pub enum EskaError {
    #[error("Error [Eska]: {0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, EskaError>;
