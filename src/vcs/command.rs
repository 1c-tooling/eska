//! Isolated system Git execution for network and mutating operations.

use std::{
    ffi::OsStr,
    fmt, io,
    path::Path,
    process::{Command, ExitStatus, Output},
};

use gix::bstr::BString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Fetch,
    Ancestry,
    UpdateBase,
    Switch,
}

pub enum Error {
    Spawn {
        operation: Operation,
        source: io::Error,
    },
    Failed {
        operation: Operation,
        status: ExitStatus,
        stderr: BString,
    },
}

impl fmt::Debug for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { operation, source } => formatter
                .debug_struct("Spawn")
                .field("operation", operation)
                .field("source", source)
                .finish(),
            Self::Failed {
                operation, status, ..
            } => formatter
                .debug_struct("Failed")
                .field("operation", operation)
                .field("status", status)
                .finish_non_exhaustive(),
        }
    }
}

/// System Git boundary for operations not implemented by the embedded repository layer.
pub struct Executor<'a> {
    work_dir: &'a Path,
}

impl<'a> Executor<'a> {
    #[must_use]
    pub const fn new(work_dir: &'a Path) -> Self {
        Self { work_dir }
    }

    /// Fetch configured refs from one validated remote without recursing into submodules.
    ///
    /// # Errors
    /// Returns a structured process error when Git cannot start or fetch fails.
    pub fn fetch(&self, remote: &str) -> Result<(), Error> {
        self.success(
            Operation::Fetch,
            ["fetch", "--no-recurse-submodules", remote],
        )
    }

    /// Test commit ancestry without parsing human-facing Git output.
    ///
    /// # Errors
    /// Exit status 1 means `false`; every other nonzero status is an operation failure.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, Error> {
        let output = self.output(
            Operation::Ancestry,
            ["merge-base", "--is-ancestor", ancestor, descendant],
        )?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(Error::Failed {
                operation: Operation::Ancestry,
                status: output.status,
                stderr: output.stderr.into(),
            }),
        }
    }

    /// Fast-forward the currently checked out base branch to its remote-tracking ref.
    ///
    /// # Errors
    /// Returns a structured process error when the fast-forward update fails.
    pub fn fast_forward_current(&self, remote_reference: &str) -> Result<(), Error> {
        self.success(
            Operation::UpdateBase,
            ["merge", "--ff-only", remote_reference],
        )
    }

    /// Fast-forward an inactive local branch without checking it out.
    ///
    /// Git itself rejects a branch checked out by another linked worktree.
    ///
    /// # Errors
    /// Returns a structured process error when the protected branch update fails.
    pub fn fast_forward_inactive(&self, branch: &str, remote_reference: &str) -> Result<(), Error> {
        self.success(
            Operation::UpdateBase,
            ["branch", "--force", branch, remote_reference],
        )
    }

    /// Create and switch to a new branch from a fully qualified local base reference.
    ///
    /// # Errors
    /// Returns a structured process error when branch creation or checkout fails.
    pub fn switch_new_branch(&self, branch: &str, base_reference: &str) -> Result<(), Error> {
        self.success(
            Operation::Switch,
            ["switch", "--no-guess", "--create", branch, base_reference],
        )
    }

    fn success<I, S>(&self, operation: Operation, args: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.output(operation, args)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(Error::Failed {
                operation,
                status: output.status,
                stderr: output.stderr.into(),
            })
        }
    }

    fn output<I, S>(&self, operation: Operation, args: I) -> Result<Output, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new("git");
        remove_repository_redirects(&mut command);
        command
            .current_dir(self.work_dir)
            .env("LC_ALL", "C")
            .args(args)
            .output()
            .map_err(|source| Error::Spawn { operation, source })
    }
}

/// Prevent inherited Git variables from redirecting an operation outside the discovered worktree.
fn remove_repository_redirects(command: &mut Command) {
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_CEILING_DIRECTORIES",
        "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    ] {
        command.env_remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn redirecting_environment_is_removed_without_dropping_credentials() {
        let mut command = Command::new("git");
        command.env("GIT_DIR", PathBuf::from("elsewhere"));
        command.env("SSH_AUTH_SOCK", PathBuf::from("agent"));

        remove_repository_redirects(&mut command);

        let environment: Vec<_> = command.get_envs().collect();
        assert!(environment.contains(&(OsStr::new("GIT_DIR"), None)));
        assert!(environment.contains(&(OsStr::new("SSH_AUTH_SOCK"), Some(OsStr::new("agent")))));
    }
}
