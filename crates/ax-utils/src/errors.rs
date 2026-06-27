//! Error types for ax.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AxError {
    #[error("file error: {0}")]
    File(#[from] FileError),
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("database error: {0}")]
    Database(#[from] DatabaseError),
    #[error("search error: {0}")]
    Search(#[from] SearchError),
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("lock unavailable: {0}")]
    LockUnavailable(String),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct FileError {
    pub message: String,
    pub path: Option<String>,
}

impl FileError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: None,
        }
    }

    pub fn with_path(message: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: Some(path.into()),
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ParseError {
    pub message: String,
    pub file_path: Option<String>,
    pub line: Option<i32>,
}

impl ParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            file_path: None,
            line: None,
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct DatabaseError {
    pub message: String,
}

impl DatabaseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct SearchError {
    pub message: String,
}

impl SearchError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ConfigError {
    pub message: String,
}

impl ConfigError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
