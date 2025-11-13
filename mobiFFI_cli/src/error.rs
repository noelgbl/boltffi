use crate::config::ConfigError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("mobiFFI.toml not found in current directory")]
    ConfigNotFound,

    #[error("no built libraries found for {platform}")]
    NoLibrariesFound { platform: String },

    #[error("command failed: {command}")]
    CommandFailed {
        command: String,
        status: Option<i32>,
    },

    #[error("tool not found: {tool}")]
    ToolNotFound { tool: String },

    #[error("failed to create directory {path}")]
    CreateDirectoryFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to copy file from {from} to {to}")]
    CopyFailed {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to read file {path}")]
    ReadFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write file {path}")]
    WriteFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("xcframework creation failed")]
    XcframeworkFailed { source: std::io::Error },

    #[error("lipo failed for simulator fat library")]
    LipoFailed { source: std::io::Error },

    #[error("zip creation failed")]
    ZipFailed { source: std::io::Error },

    #[error("missing rust target: {target}")]
    MissingTarget { target: String },
}

pub type Result<T> = std::result::Result<T, CliError>;
