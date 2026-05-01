use std::path::PathBuf;
use thiserror::Error;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum CodescoutError {
    #[error("not a git repository: {0}")]
    NotGitRepo(PathBuf),

    #[error("shadow directory not found: {0}")]
    ShadowNotFound(PathBuf),

    #[error("shadow already initialised at {0}; use `rebuild` to start fresh")]
    ShadowExists(PathBuf),

    #[error("git command failed: {stderr}")]
    GitFailed { stderr: String },

    #[error("claude invocation failed: {0}")]
    ClaudeFailed(String),

    #[error("malformed map file at {path}: {reason}")]
    MalformedMap { path: PathBuf, reason: String },

    #[error("home directory not found")]
    NoHomeDir,
}

pub type Result<T> = anyhow::Result<T>;
