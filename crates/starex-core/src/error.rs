use std::path::{Path, PathBuf};

/// Errors returned by `starex-core`. Every variant renders as a sentence a user can act on.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is not valid Starex data: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("This game is no longer in your library.")]
    GameNotFound,

    #[error("{0}")]
    Invalid(String),
}

impl Error {
    pub(crate) fn io(action: &'static str, path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
