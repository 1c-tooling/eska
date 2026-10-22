//! Explicit repository-wide shelf commands and their localized arguments.

use eska_presentation::arguments::vcs::shelves::MutationOptions;
pub use eska_presentation::arguments::vcs::shelves::{
    ShelveArgs, ShelvesArgs, UnshelveArgs, localize,
};

use crate::{
    cli::{localization::Localizer, shelves as presentation},
    vcs::shelves::{self, Session},
};
use std::{path::Path, process::ExitCode};

struct ShelveHandler<'a> {
    args: &'a ShelveArgs,
}

impl ShelveHandler<'_> {
    /// Save the current branch's repository-wide state or display its exact preview.
    #[must_use]
    pub fn run(&self, start: &Path, localizer: &Localizer) -> ExitCode {
        MutationOptionsHandler {
            args: &self.args.options,
        }
        .run(start, false, None, localizer)
    }
}
struct UnshelveHandler<'a> {
    args: &'a UnshelveArgs,
}

impl UnshelveHandler<'_> {
    /// Restore the current branch's matching shelf without overwriting conflicts.
    #[must_use]
    pub fn run(&self, start: &Path, localizer: &Localizer) -> ExitCode {
        MutationOptionsHandler {
            args: &self.args.options,
        }
        .run(start, true, self.args.id.as_deref(), localizer)
    }
}
struct MutationOptionsHandler<'a> {
    args: &'a MutationOptions,
}

impl MutationOptionsHandler<'_> {
    /// Dispatch one preflighted mutation with a shared read-only preview and error contract.
    fn run(
        &self,
        start: &Path,
        restore: bool,
        id: Option<&str>,
        localizer: &Localizer,
    ) -> ExitCode {
        let repository = match presentation::repository(start, self.args.format, localizer) {
            Ok(repo) => repo,
            Err(code) => return code,
        };
        let result = if self.args.dry_run {
            if restore {
                shelves::restore_plan(&repository, id)
            } else {
                shelves::plan(&repository)
            }
        } else {
            Session::acquire(&repository).and_then(|session| {
                if restore {
                    session.restore(id)
                } else {
                    session.capture()
                }
            })
        };
        match result {
            Ok(saved) => presentation::write_result(
                self.args.format,
                &saved,
                restore,
                self.args.dry_run,
                localizer,
            ),
            Err(error) => presentation::write_error(self.args.format, &error, localizer),
        }
    }
}
struct ShelvesHandler<'a> {
    args: &'a ShelvesArgs,
}

impl ShelvesHandler<'_> {
    /// List retained shelves without modifying the repository or filesystem.
    #[must_use]
    pub fn run(&self, start: &Path, localizer: &Localizer) -> ExitCode {
        let repository = match presentation::repository(start, self.args.format, localizer) {
            Ok(repo) => repo,
            Err(code) => return code,
        };
        match shelves::list(&repository) {
            Ok(saved) => presentation::write_list(self.args.format, &saved, localizer),
            Err(error) => presentation::write_error(self.args.format, &error, localizer),
        }
    }
}

/// Execute parsed shelf options in the repository owner.
#[must_use]
pub fn shelve(args: &ShelveArgs, start: &Path, localizer: &Localizer) -> ExitCode {
    ShelveHandler { args }.run(start, localizer)
}

/// Execute parsed shelf options in the repository owner.
#[must_use]
pub fn unshelve(args: &UnshelveArgs, start: &Path, localizer: &Localizer) -> ExitCode {
    UnshelveHandler { args }.run(start, localizer)
}

/// Execute parsed shelf options in the repository owner.
#[must_use]
pub fn list(args: &ShelvesArgs, start: &Path, localizer: &Localizer) -> ExitCode {
    ShelvesHandler { args }.run(start, localizer)
}
