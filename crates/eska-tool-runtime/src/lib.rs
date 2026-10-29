//! Lightweight component discovery and owned process I/O; no domain engines or localization.

mod capture;
pub mod contract;
mod exchange;
pub mod installation;
mod process;
mod query;
mod resolver;
pub mod vcs;

pub use capture::{capture, capture_input};
pub use exchange::{Operation, exchange};
pub use process::delegate;
pub use query::{QueryPolicy, query, query_with_policy};
pub use resolver::{resolve, resolve_at};

use std::{io, path::PathBuf};

#[derive(Clone, Copy, Debug)]
pub enum Component {
    Project,
    Check,
    Fmt,
    Build,
    Vcs,
}

impl Component {
    /// Keep executable discovery independent of localized help or command aliases.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Project => "eska-project",
            Self::Check => "eska-check",
            Self::Fmt => "eska-fmt",
            Self::Build => "eska-build",
            Self::Vcs => "eska-vcs",
        }
    }

    /// Explicit paths always take precedence and never silently fall back after failure.
    #[must_use]
    pub const fn variable(self) -> &'static str {
        match self {
            Self::Project => "ESKA_PROJECT_BINARY",
            Self::Check => "ESKA_CHECK_BINARY",
            Self::Fmt => "ESKA_FMT_BINARY",
            Self::Build => "ESKA_BUILD_BINARY",
            Self::Vcs => "ESKA_VCS_BINARY",
        }
    }
}

/// Structured component failures never imply that a repository or source is clean.
#[derive(Debug)]
pub enum Error {
    Missing {
        component: Component,
    },
    Invalid {
        component: Component,
        path: PathBuf,
        source: io::Error,
    },
    Selection {
        component: Component,
        path: PathBuf,
    },
    Incompatible {
        tool: String,
        receipt: PathBuf,
    },
    Signal(io::Error),
    Process {
        component: Component,
        source: io::Error,
    },
    Timeout,
    Cancelled,
    InvalidResponse,
    OutputTooLarge,
    Failed,
    Rejected {
        component: Component,
        code: String,
        detail: Option<String>,
    },
}

impl Error {
    /// Stable internal categories can be localized by each consuming CLI.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Missing { .. } => "component_missing",
            Self::Invalid { .. } | Self::Selection { .. } => "component_invalid",
            Self::Incompatible { .. } => "component_incompatible",
            Self::Signal(_) => "signal_handler",
            Self::Process { .. } => "component_process",
            Self::Timeout => "component_timeout",
            Self::Cancelled => "cancelled",
            Self::InvalidResponse => "component_response_invalid",
            Self::OutputTooLarge => "component_output_too_large",
            Self::Failed => "component_failed",
            Self::Rejected { .. } => "component_rejected",
        }
    }
}

#[cfg(all(test, unix))]
#[path = "../../../tests/support/mod.rs"]
mod test_support;
