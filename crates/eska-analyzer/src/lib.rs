//! Integration with the published analyzer executable; no BSL engine is linked or copied.

pub mod check;
pub mod configuration;
pub mod migration;
pub mod process;
mod storage;

#[cfg(test)]
#[path = "../../../tests/support/mod.rs"]
mod test_support;

use std::{io, path::PathBuf};

use eska_project_core::application::ContextError;

/// Process and configuration failures preserve machine categories and relevant coordinates.
#[derive(Debug)]
pub enum Error {
    Context(Box<ContextError>),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    InvalidToml {
        path: PathBuf,
        detail: String,
    },
    InvalidPolicy {
        field: String,
    },
    NonPortablePath {
        path: PathBuf,
    },
    InputTooLarge {
        path: PathBuf,
    },
    ManagedConflict {
        path: PathBuf,
    },
    MigrationRequired {
        path: PathBuf,
    },
    MigrationNotRequired,
    ContextConflict {
        current: Option<String>,
        requested: Option<String>,
    },
    Busy {
        path: PathBuf,
    },
    StalePreview,
    MigrationConflicts {
        fields: Vec<String>,
    },
    ProcessStart {
        binary: PathBuf,
        source: io::Error,
    },
    ProcessIo(io::Error),
    Timeout,
    Cancelled,
    OutputTooLarge,
    AnalyzerVersion {
        actual: String,
    },
    AnalyzerContract,
    CheckConfig {
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
    },
    Rollback {
        paths: Vec<PathBuf>,
        original: Box<Self>,
    },
}

impl Error {
    /// Return stable categories shared by project CLI, Metadata API and CI consumers.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Context(_) => "context",
            Self::Io { .. } => "io",
            Self::InvalidToml { .. } => "invalid_toml",
            Self::InvalidPolicy { .. } => "invalid_policy",
            Self::NonPortablePath { .. } => "nonportable_path",
            Self::InputTooLarge { .. } => "input_too_large",
            Self::ManagedConflict { .. } => "managed_conflict",
            Self::MigrationRequired { .. } => "migration_required",
            Self::MigrationNotRequired => "migration_not_required",
            Self::ContextConflict { .. } => "context_conflict",
            Self::Busy { .. } => "busy",
            Self::StalePreview => "stale_preview",
            Self::MigrationConflicts { .. } => "migration_conflicts",
            Self::ProcessStart { .. } => "process_start",
            Self::ProcessIo(_) => "process_io",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::OutputTooLarge => "output_too_large",
            Self::AnalyzerVersion { .. } => "analyzer_version",
            Self::AnalyzerContract => "analyzer_contract",
            Self::CheckConfig { .. } => "check_config",
            Self::Rollback { .. } => "rollback_incomplete",
        }
    }
}
