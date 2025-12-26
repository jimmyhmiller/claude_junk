use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Not initialized. Run 'init' first.")]
    NotInitialized,

    #[error("Not authenticated. Run 'login' first.")]
    NotAuthenticated,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Sync error: {0}")]
    Sync(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Server error: {0}")]
    Server(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
