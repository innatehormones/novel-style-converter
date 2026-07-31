#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]        Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]              Io(#[from] std::io::Error),
    #[error("http error: {0}")]            Http(#[from] reqwest::Error),
    #[error("ai provider: {0}")]           Ai(String),
    #[error("splitting: {0}")]             Splitter(String),
    #[error("validation: {0}")]            Validation(String),
    #[error("not found: {0}")]             NotFound(String),
    #[error("serde: {0}")]                 Serde(#[from] serde_json::Error),
    #[error("{0}")]                        Other(String),
}

impl Error {
    pub fn msg(s: impl Into<String>) -> Self { Self::Other(s.into()) }
}

pub type Result<T> = std::result::Result<T, Error>;