use thiserror::Error;

#[derive(Error, Debug)]
pub enum DataProtocolError {
    #[error("P2P network error: {0}")]
    Network(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Identity error: {0}")]
    Identity(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Dataset not found: {0}")]
    NotFound(String),

    #[error("Payment error: {0}")]
    Payment(String),

    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, DataProtocolError>;
