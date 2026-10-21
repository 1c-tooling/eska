//! The native owner reports machine and selected-project prerequisites without changing files.

use crate::cli::{diagnostics, localization::Localizer, platform};
use clap::Args;
use eska_project_core::{
    doctor::{Check, CheckStatus},
    project::{
        discovery,
        selection::{SelectionIntent, select_projects},
    },
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    ibcmd: Option<PathBuf>,
    #[arg(long)]
    platform_arch: Option<String>,
    #[arg(long)]
    distrobox: Option<String>,
    #[arg(short = 'p', long)]
    project: Vec<String>,
    #[arg(long)]
    workspace: bool,
}

impl DoctorArgs {
    /// Keep machine checks available even when project discovery or selection is invalid.
    #[must_use]
    pub fn inspect(&self, root: &Path, localizer: &Localizer) -> Value {
        let options = platform::tool_options(
            self.ibcmd.clone(),
            self.platform_arch.clone(),
            self.distrobox.clone(),
        );
        let (machine, detail) = match &options {
            Ok(_) => (
                Check::new(
                    "machine.config",
                    CheckStatus::Pass,
                    "valid",
                    &["platform", "build", "patch"],
                ),
                None,
            ),
            Err(error) => (
                Check::new(
                    "machine.config",
                    CheckStatus::Fail,
                    "invalid",
                    &["platform", "build", "patch"],
                )
                .with_remediation("correct-machine-config"),
                Some(diagnostics::present_global_config_error(error, localizer)),
            ),
        };
        let mut checks = vec![machine];
        let mut details = vec![detail];
        if let Ok(context) = discovery::discover_context(root)
            && let Ok(selection) = select_projects(
                &context,
                &self.project,
                self.workspace,
                SelectionIntent::ReadOnly,
            )
        {
            for selected in selection.projects() {
                let native = eska_build_runtime::doctor::inspect(
                    selected.project(),
                    selected
                        .name()
                        .map(eska_project_core::project::ProjectName::as_str),
                    options.as_ref().ok(),
                );
                details.extend((0..native.len()).map(|_| None::<String>));
                checks.extend(native);
            }
        }
        json!({"checks":checks,"details":details})
    }
}
