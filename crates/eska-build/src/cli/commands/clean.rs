//! Removal of reusable build infobases without touching published artifacts.

pub use eska_presentation::arguments::build::clean::{CleanArgs, localize};

use std::{path::Path, process::ExitCode};

use crate::{
    cli::{diagnostics, localization::Localizer},
    project::{
        ProjectName,
        build::{InfobaseCleanOutcome, clean_infobase, managed_infobase_root},
        discovery::{self, DiscoveryContext},
        selection::{SelectionIntent, select_projects},
    },
};

struct CleanHandler<'a> {
    options: &'a CleanArgs,
}

impl CleanHandler<'_> {
    /// Remove managed infobases for the selected project scope.
    pub fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let context = match discovery::discover_context(project_dir) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("{}", diagnostics::present_context_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let selection = match select_projects(
            &context,
            &self.options.project,
            self.options.workspace,
            SelectionIntent::MultipleMutation,
        ) {
            Ok(selection) => selection,
            Err(error) => {
                eprintln!(
                    "{}",
                    diagnostics::present_selection_error(&error, localizer)
                );
                return ExitCode::FAILURE;
            }
        };
        let workspace_root = match &context {
            DiscoveryContext::Standalone(_) => None,
            DiscoveryContext::Workspace { workspace, .. } => Some(workspace.root()),
        };
        let mut failed = false;
        for selected in selection.projects() {
            let workspace = selected
                .name()
                .zip(workspace_root)
                .map(|(name, root)| (root, name.as_str()));
            let Ok(root) = managed_infobase_root(selected.project(), workspace) else {
                write_project_error(
                    selected.name(),
                    &localizer.text("clean-project-name-missing"),
                    localizer,
                );
                failed = true;
                continue;
            };
            let scope = workspace_root.unwrap_or_else(|| selected.project().root());
            match clean_infobase(&root, scope) {
                Ok(outcome) => write_outcome(selected.name(), &root, outcome, localizer),
                Err(error) => {
                    write_project_error(
                        selected.name(),
                        &diagnostics::present_managed_infobase_error(&error, localizer),
                        localizer,
                    );
                    failed = true;
                }
            }
        }
        if failed {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    }
}

fn write_outcome(
    name: Option<&ProjectName>,
    root: &Path,
    outcome: InfobaseCleanOutcome,
    localizer: &Localizer,
) {
    let key = match outcome {
        InfobaseCleanOutcome::Removed => "clean-removed",
        InfobaseCleanOutcome::AlreadyAbsent => "clean-absent",
    };
    let detail = localizer.format(
        key,
        &[(
            "path",
            crate::cli::localization::LocalizationValue::Text(&display_path(root)),
        )],
    );
    if let Some(name) = name {
        println!(
            "{}",
            localizer.format(
                "clean-member-result",
                &[
                    (
                        "name",
                        crate::cli::localization::LocalizationValue::Text(name.as_str()),
                    ),
                    (
                        "result",
                        crate::cli::localization::LocalizationValue::Text(&detail),
                    ),
                ],
            )
        );
    } else {
        println!("{detail}");
    }
}

fn write_project_error(name: Option<&ProjectName>, detail: &str, localizer: &Localizer) {
    if let Some(name) = name {
        eprintln!(
            "{}",
            localizer.format(
                "clean-member-error",
                &[
                    (
                        "name",
                        crate::cli::localization::LocalizationValue::Text(name.as_str()),
                    ),
                    (
                        "reason",
                        crate::cli::localization::LocalizationValue::Text(detail),
                    ),
                ],
            )
        );
    } else {
        eprintln!("{detail}");
    }
}

/// Escape control characters before displaying a local path.
fn display_path(path: &Path) -> String {
    let mut display = String::new();
    for character in path.to_string_lossy().chars() {
        if character.is_control() {
            display.extend(character.escape_default());
        } else {
            display.push(character);
        }
    }
    display
}

/// Execute parsed cleanup options without linking its runner into the shell.
pub fn run(args: &CleanArgs, directory: &Path, localizer: &Localizer) -> ExitCode {
    CleanHandler { options: args }.run(directory, localizer)
}
