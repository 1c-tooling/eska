//! Localized presentation of the read-only project environment diagnosis.

use eska_presentation::arguments::project::doctor::OutputFormat;
pub use eska_presentation::arguments::project::doctor::{DoctorArgs, localize};

use std::{
    io::{self, IsTerminal},
    path::Path,
    process::ExitCode,
};

use serde::Serialize;

use eska_presentation::{
    diagnostics,
    localization::{LocalizationValue, Localizer},
};
use eska_project_core::{
    doctor::{self, Check, CheckStatus, Report, Summary},
    project::{
        ProjectName,
        discovery::{self, DiscoveryContext},
        selection::{SelectionIntent, select_projects},
    },
};
mod client;

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

struct DoctorHandler<'a> {
    args: &'a DoctorArgs,
}

impl DoctorHandler<'_> {
    /// Discover the selected project scope and run every independent read-only check.
    pub fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let mut report = Report::default();
        let mut details = Vec::new();
        let native = client::native(project_dir, &self.native_options(localizer), localizer);
        native.append_machine(&mut report, &mut details);
        let diagnosis = match discovery::discover_context(project_dir) {
            Ok(context) => self.inspect_context(&context, &native, report, details, localizer),
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
                client::vcs(project_dir, None, &[project_dir.to_path_buf()], localizer)
                    .append(&mut report, &mut details);
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

    /// Preserve native option bytes and selectors while the build owner resolves machine settings.
    fn native_options(&self, localizer: &Localizer) -> Vec<std::ffi::OsString> {
        let mut options = vec!["--lang".into(), localizer.locale().as_str().into()];
        for (flag, value) in [
            ("--ibcmd", self.args.ibcmd.as_deref().map(Path::as_os_str)),
            (
                "--platform-arch",
                self.args.platform_arch.as_deref().map(std::ffi::OsStr::new),
            ),
            (
                "--distrobox",
                self.args.distrobox.as_deref().map(std::ffi::OsStr::new),
            ),
        ] {
            if let Some(value) = value {
                options.extend([flag.into(), value.to_owned()]);
            }
        }
        for project in &self.args.project {
            options.extend(["--project".into(), project.into()]);
        }
        if self.args.workspace {
            options.push("--workspace".into());
        }
        options
    }

    /// Inspect the selected members plus their shared repository and workflow context.
    fn inspect_context(
        &self,
        context: &DiscoveryContext,
        native: &client::Packet,
        mut report: Report,
        mut details: Vec<Option<String>>,
        localizer: &Localizer,
    ) -> Diagnosis {
        let selection = match select_projects(
            context,
            &self.args.project,
            self.args.workspace,
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
                ),
            );
            native
                .for_project(
                    selected.project(),
                    selected.name().map(ProjectName::as_str),
                    localizer,
                )
                .append(&mut report, &mut details);
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
            client::vcs(
                context_root(context),
                Some(first.project().root()),
                &attribute_roots,
                localizer,
            )
            .append(&mut report, &mut details);
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
        match self.args.format {
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
        if let Some(remediation) = &check.remediation {
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
    let source = check.source.as_deref().unwrap_or("—");
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

/// Compose diagnostics for parsed project options using independent owners.
pub fn run(args: &DoctorArgs, directory: &Path, localizer: &Localizer) -> ExitCode {
    DoctorHandler { args }.run(directory, localizer)
}

#[cfg(test)]
mod tests {
    use super::{Check, CheckStatus, Localizer, Report, render_human};
    use eska_presentation::localization::Locale;

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
