use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] tokio_postgres::Error),
    #[error("could not get a database connection: {0}")]
    Pool(#[from] deadpool_postgres::PoolError),
    #[error("could not set up the connection: {0}")]
    Setup(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("download failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("not connected to a database")]
    NotConnected,
    #[error("{0}")]
    NotNereus(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
