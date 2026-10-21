//! Arguments, diagnostics and help for `eska clone`.

pub use eska_presentation::arguments::vcs::clone::{CloneArgs, localize};

use std::{path::Path, process::ExitCode};

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::clone::{self, CloneError},
};

struct CloneHandler<'a> {
    args: &'a CloneArgs,
}

impl CloneHandler<'_> {
    /// Validate and execute one clone request.
    #[must_use]
    pub fn run(&self, base: &Path, localizer: &Localizer) -> ExitCode {
        let plan = match clone::inspect(
            base,
            &self.args.repository,
            self.args.directory.as_deref(),
            &self.args.remote,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                eprintln!("{}", present(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        match clone::execute(plan) {
            Ok(project) => {
                println!(
                    "{}",
                    localizer.format(
                        "clone-created",
                        &[(
                            "path",
                            LocalizationValue::Text(&project.root().to_string_lossy()),
                        )],
                    )
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{}", present(&error, localizer));
                ExitCode::FAILURE
            }
        }
    }
}

fn present(error: &CloneError, localizer: &Localizer) -> String {
    match error {
        CloneError::RepositoryUrl(_) => localizer.text("clone-repository-invalid"),
        CloneError::LocalRepository { path, .. } => {
            path_message(localizer, "clone-local-repository-invalid", path)
        }
        CloneError::RemoteName(_) => localizer.text("clone-remote-invalid"),
        CloneError::MissingDirectoryName => localizer.text("clone-directory-required"),
        CloneError::InvalidBase { path } => path_message(localizer, "clone-base-invalid", path),
        CloneError::InvalidDestination { path } => {
            path_message(localizer, "clone-destination-invalid", path)
        }
        CloneError::AlreadyExists { path } => {
            path_message(localizer, "clone-destination-exists", path)
        }
        CloneError::Io { path, .. } => path_message(localizer, "clone-io-error", path),
        CloneError::Network(error) => match error {
            crate::vcs::network::CloneError::Prepare(_)
            | crate::vcs::network::CloneError::RemoteName(_)
            | crate::vcs::network::CloneError::Fetch(_) => localizer.text("clone-fetch-error"),
            crate::vcs::network::CloneError::Checkout(_) => localizer.text("clone-checkout-error"),
        },
        CloneError::IncompleteCheckout { collisions, errors } => localizer.format(
            "clone-checkout-incomplete",
            &[
                (
                    "collisions",
                    LocalizationValue::Number(i64::try_from(*collisions).unwrap_or(i64::MAX)),
                ),
                (
                    "errors",
                    LocalizationValue::Number(i64::try_from(*errors).unwrap_or(i64::MAX)),
                ),
            ],
        ),
        CloneError::Validation(error) => diagnostics::present_project_error(error, localizer),
        CloneError::Rollback { path, original, .. } => localizer.format(
            "clone-rollback-error",
            &[
                (
                    "reason",
                    LocalizationValue::Text(&present(original, localizer)),
                ),
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
            ],
        ),
    }
}

fn path_message(localizer: &Localizer, key: &str, path: &Path) -> String {
    localizer.format(
        key,
        &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
    )
}

/// Execute parsed VCS options in the repository owner.
#[must_use]
pub fn run(args: &CloneArgs, directory: &Path, localizer: &Localizer) -> ExitCode {
    CloneHandler { args }.run(directory, localizer)
}
