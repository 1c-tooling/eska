//! Default behavior of bare `eska`; this is not a separate CLI subcommand.

use std::{path::Path, process::ExitCode};

use crate::{
    cli::{diagnostics, localization::Localizer},
    project::discovery,
};

pub(super) fn run(project_dir: &Path, localizer: &Localizer) -> ExitCode {
    match discovery::discover_context(project_dir) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", diagnostics::present_context_error(&error, localizer));
            ExitCode::FAILURE
        }
    }
}
