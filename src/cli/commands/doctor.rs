//! Localized presentation of the read-only project environment diagnosis.

use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Args, ValueEnum};
use serde::Serialize;

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
        platform,
    },
    project::{
        ProjectName,
        discovery::{self, DiscoveryContext},
        doctor::{self, Check, CheckStatus, Report, Summary},
        selection::{SelectionIntent, select_projects},
    },
};

#[derive(Debug, Args)]
pub(in crate::cli) struct DoctorArgs {
    #[arg(long)]
    ibcmd: Option<PathBuf>,

    #[arg(long)]
    platform_arch: Option<String>,

    #[arg(long)]
    distrobox: Option<String>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[arg(short = 'p', long)]
    project: Vec<String>,

    #[arg(long)]
    workspace: bool,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Serialize)]
struct DoctorDocument<'a> {
    schema_version: u8,
    status: &'static str,
    scope: &'a ScopeDocument,
    checks: &'a [Check],
    summary: Summary,
}

#[derive(Serialize)]
struct ScopeDocument {
    kind: &'static str,
    projects: Vec<String>,
}

impl Default for ScopeDocument {
    /// Represent a scope that could not be resolved from an invalid manifest.
    fn default() -> Self {
        Self {
            kind: "unresolved",
            projects: Vec::new(),
        }
    }
}

struct Diagnosis {
    report: Report,
    details: Vec<Option<String>>,
    scope: ScopeDocument,
}

impl DoctorArgs {
    /// Discover the selected project scope and run every independent read-only check.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let mut report = Report::default();
        let mut details = Vec::new();
        let tool_options = self.inspect_machine_config(&mut report, &mut details, localizer);
        let diagnosis = match discovery::discover_context(project_dir) {
            Ok(context) => {
                self.inspect_context(&context, tool_options.as_ref(), report, details, localizer)
            }
            Err(error) => {
                push(
                    &mut report,
                    &mut details,
                    Check::new(
                        "project.config",
                        CheckStatus::Fail,
                        "invalid",
                        &["status", "diff", "save", "build"],
                    )
                    .with_remediation("correct-project-config"),
                    Some(diagnostics::present_context_error(&error, localizer)),
                );
                extend(
                    &mut report,
                    &mut details,
                    doctor::inspect_system_git(project_dir, &[project_dir.to_path_buf()]),
                );
                Diagnosis {
                    report,
                    details,
                    scope: ScopeDocument::default(),
                }
            }
        };
        self.render(
            &diagnosis.report,
            &diagnosis.details,
            &diagnosis.scope,
            localizer,
        )
    }

    /// Validate machine-level runner settings while allowing project checks to continue.
    fn inspect_machine_config(
        &self,
        report: &mut Report,
        details: &mut Vec<Option<String>>,
        localizer: &Localizer,
    ) -> Option<crate::project::build::ToolOptions> {
        match platform::tool_options(
            self.ibcmd.clone(),
            self.platform_arch.clone(),
            self.distrobox.clone(),
        ) {
            Ok(options) => {
                push(
                    report,
                    details,
                    Check::new(
                        "machine.config",
                        CheckStatus::Pass,
                        "valid",
                        &["platform", "build", "patch"],
                    ),
                    None,
                );
                Some(options)
            }
            Err(error) => {
                push(
                    report,
                    details,
                    Check::new(
                        "machine.config",
                        CheckStatus::Fail,
                        "invalid",
                        &["platform", "build", "patch"],
                    )
                    .with_remediation("correct-machine-config"),
                    Some(diagnostics::present_global_config_error(&error, localizer)),
                );
                None
            }
        }
    }

    /// Inspect the selected members plus their shared repository and workflow context.
    fn inspect_context(
        &self,
        context: &DiscoveryContext,
        tool_options: Option<&crate::project::build::ToolOptions>,
        mut report: Report,
        mut details: Vec<Option<String>>,
        localizer: &Localizer,
    ) -> Diagnosis {
        let selection = match select_projects(
            context,
            &self.project,
            self.workspace,
            SelectionIntent::ReadOnly,
        ) {
            Ok(selection) => selection,
            Err(error) => {
                push(
                    &mut report,
                    &mut details,
                    Check::new(
                        "project.selection",
                        CheckStatus::Fail,
                        "invalid",
                        &["doctor"],
                    )
                    .with_remediation("correct-project-selection"),
                    Some(diagnostics::present_selection_error(&error, localizer)),
                );
                return Diagnosis {
                    report,
                    details,
                    scope: scope(context, false, &[]),
                };
            }
        };

        for selected in selection.projects() {
            extend(
                &mut report,
                &mut details,
                doctor::inspect_project(
                    selected.project(),
                    selected.name().map(ProjectName::as_str),
                    tool_options,
                ),
            );
        }
        if let Some(first) = selection.projects().first() {
            let mut attribute_roots = selection
                .projects()
                .iter()
                .map(|selected| selected.project().root().to_path_buf())
                .collect::<Vec<_>>();
            if let DiscoveryContext::Workspace { workspace, .. } = &context {
                attribute_roots.push(workspace.root().to_path_buf());
            }
            extend(
                &mut report,
                &mut details,
                doctor::inspect_vcs(context_root(context), first.project(), &attribute_roots),
            );
        }
        let names = selection
            .projects()
            .iter()
            .filter_map(|selected| selected.name().map(ToString::to_string))
            .collect::<Vec<_>>();
        Diagnosis {
            report,
            details,
            scope: scope(context, selection.is_aggregate(), &names),
        }
    }

    /// Render the complete report and derive the process result from failed checks only.
    fn render(
        &self,
        report: &Report,
        details: &[Option<String>],
        scope: &ScopeDocument,
        localizer: &Localizer,
    ) -> ExitCode {
        match self.format {
            OutputFormat::Human => println!(
                "{}",
                render_human(report, details, localizer, styling_enabled())
            ),
            OutputFormat::Json => {
                let document = DoctorDocument {
                    schema_version: 1,
                    status: if report.has_failures() { "error" } else { "ok" },
                    scope,
                    checks: report.checks(),
                    summary: report.summary(),
                };
                let Ok(json) = serde_json::to_string_pretty(&document) else {
                    eprintln!("{}", localizer.text("doctor-json-error"));
                    return ExitCode::FAILURE;
                };
                println!("{json}");
            }
        }
        if report.has_failures() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    }
}

/// Keep human-only detailed diagnostics aligned with their machine-facing checks.
fn push(
    report: &mut Report,
    details: &mut Vec<Option<String>>,
    check: Check,
    detail: Option<String>,
) {
    report.push(check);
    details.push(detail);
}

/// Add core checks that do not carry localized details.
fn extend(report: &mut Report, details: &mut Vec<Option<String>>, checks: Vec<Check>) {
    for check in checks {
        push(report, details, check, None);
    }
}

fn context_root(context: &DiscoveryContext) -> &Path {
    match context {
        DiscoveryContext::Standalone(project) => project.root(),
        DiscoveryContext::Workspace { workspace, .. } => workspace.root(),
    }
}

/// Describe whether diagnosis covers one project or all workspace members.
fn scope(context: &DiscoveryContext, aggregate: bool, projects: &[String]) -> ScopeDocument {
    ScopeDocument {
        kind: match context {
            DiscoveryContext::Workspace { .. } if aggregate => "workspace",
            DiscoveryContext::Workspace { .. } | DiscoveryContext::Standalone(_) => "project",
        },
        projects: projects.to_vec(),
    }
}

/// Render checks in a compact table with an actionable line for every warning or failure.
fn render_human(
    report: &Report,
    details: &[Option<String>],
    localizer: &Localizer,
    styled: bool,
) -> String {
    let mut lines = vec![localizer.text("doctor-title")];
    for (check, detail) in report.checks().iter().zip(details) {
        let marker = style_marker(check.status, styled);
        let label = localizer.text(&format!("doctor-check-{}", check.id.replace('.', "-")));
        let message = detail
            .clone()
            .unwrap_or_else(|| check_message(check, localizer));
        let owner = check
            .project
            .as_deref()
            .map_or_else(String::new, |project| format!(" [{project}]"));
        lines.push(format!("{marker} {label}{owner}: {message}"));
        if let Some(remediation) = check.remediation {
            lines.push(format!(
                "  {}: {}",
                localizer.text("doctor-fix-label"),
                localizer.text(&format!("doctor-fix-{remediation}"))
            ));
        }
    }
    let summary = report.summary();
    lines.push(String::new());
    lines.push(localizer.format(
        "doctor-summary",
        &[
            ("passed", number(summary.passed)),
            ("warnings", number(summary.warnings)),
            ("failed", number(summary.failed)),
            ("skipped", number(summary.skipped)),
        ],
    ));
    lines.join("\n")
}

/// Color the status marker while keeping labels and diagnostics easy to copy.
fn style_marker(status: CheckStatus, styled: bool) -> String {
    let (marker, color) = match status {
        CheckStatus::Pass => ("✓", "1;32"),
        CheckStatus::Warning => ("!", "1;33"),
        CheckStatus::Fail => ("✗", "1;31"),
        CheckStatus::Skipped => ("–", "2;90"),
    };
    if styled {
        format!("\x1b[{color}m{marker}\x1b[0m")
    } else {
        marker.to_owned()
    }
}

/// Enable colors only for an interactive stdout that permits terminal styling.
fn styling_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Format the stable check code with locale-specific explanatory text.
fn check_message(check: &Check, localizer: &Localizer) -> String {
    let expected = check.expected.as_deref().unwrap_or("—");
    let actual = check.actual.as_deref().unwrap_or("—");
    let source = check.source.unwrap_or("—");
    let commands = check.commands.join(", ");
    localizer.format(
        &format!("doctor-code-{}", check.code),
        &[
            ("expected", LocalizationValue::Text(expected)),
            ("actual", LocalizationValue::Text(actual)),
            ("source", LocalizationValue::Text(source)),
            ("commands", LocalizationValue::Text(&commands)),
        ],
    )
}

/// Convert summary counters into the localization value type without overflow.
fn number(value: usize) -> LocalizationValue<'static> {
    LocalizationValue::Number(i64::try_from(value).unwrap_or(i64::MAX))
}

/// Localize command help without changing option names or machine-facing values.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("doctor-about"))
        .override_usage(localizer.text("doctor-usage"))
        .mut_arg("ibcmd", |arg| {
            arg.help(localizer.text("build-ibcmd-help"))
                .value_name(localizer.text("build-ibcmd-value"))
        })
        .mut_arg("platform_arch", |arg| {
            arg.help(localizer.text("build-arch-help"))
                .value_name(localizer.text("build-arch-value"))
        })
        .mut_arg("distrobox", |arg| {
            arg.help(localizer.text("build-distrobox-help"))
                .value_name(localizer.text("build-distrobox-value"))
        })
        .mut_arg("format", |arg| {
            arg.help(localizer.text("doctor-format-help"))
                .value_name(localizer.text("doctor-format-value"))
        })
        .mut_arg("project", |arg| {
            arg.help(localizer.text("doctor-project-help"))
                .value_name(localizer.text("doctor-project-value"))
        })
        .mut_arg("workspace", |arg| {
            arg.help(localizer.text("doctor-workspace-help"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}

#[cfg(test)]
mod tests {
    use super::{Check, CheckStatus, Localizer, Report, render_human};
    use crate::cli::localization::Locale;

    #[test]
    /// Color every status marker without changing copied or redirected report text.
    fn human_report_styles_status_markers_and_preserves_plain_output() {
        let localizer = Localizer::try_new(Locale::EnUs).expect("locale");
        let mut report = Report::default();
        let mut details = Vec::new();
        for status in [
            CheckStatus::Pass,
            CheckStatus::Warning,
            CheckStatus::Fail,
            CheckStatus::Skipped,
        ] {
            report.push(Check::new("machine.config", status, "valid", &["doctor"]));
            details.push(None);
        }

        let plain = render_human(&report, &details, &localizer, false);
        let styled = render_human(&report, &details, &localizer, true);

        assert!(!plain.contains('\x1b'), "{plain:?}");
        for sequence in ["\x1b[1;32m✓", "\x1b[1;33m!", "\x1b[1;31m✗", "\x1b[2;90m–"] {
            assert!(styled.contains(sequence), "{styled:?}");
        }
        assert_eq!(
            styled
                .replace("\x1b[1;32m", "")
                .replace("\x1b[1;33m", "")
                .replace("\x1b[1;31m", "")
                .replace("\x1b[2;90m", "")
                .replace("\x1b[0m", ""),
            plain
        );
    }
}
