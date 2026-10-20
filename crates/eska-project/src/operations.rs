//! Standalone filesystem operations share the exact project writers used by the full CLI.

use std::{
    convert::Infallible,
    path::{Path, PathBuf},
    process::ExitCode,
};

use eska_presentation::{
    diagnostics,
    localization::{LocalizationValue, Localizer},
    path_output,
};
use eska_project_core::{
    config::{FILE_NAME, ProjectConfig},
    project::{
        Project, ProjectName, ProjectType,
        build::{BuildSettings, DEFAULT_ARTIFACTS_DIRECTORY},
        create,
        designer_source::{self, SourceError},
        init, onboarding,
    },
};
use serde_json::{Value, json};

use crate::{
    cli::{self, InitArgs, NewArgs, OutputFormat, ValidateArgs},
    response::{self, Failure},
};

/// Validate manifests, selected root descriptors and their containment without loading BSL text.
pub fn validate(start: &Path, args: &ValidateArgs, localizer: &Localizer) -> ExitCode {
    let sources = match designer_source::open_projects(start, &args.project, args.workspace) {
        Ok(sources) if !sources.is_empty() => sources,
        Ok(_) => {
            return Failure::new(
                "no_projects",
                json!({}),
                localizer.text("project-empty-workspace"),
            )
            .emit(args.format);
        }
        Err(SourceError::Discovery(error)) => {
            return response::discovery(&error, localizer).emit(args.format);
        }
        Err(SourceError::Selection(error)) => {
            return Failure::new(
                "project_selection",
                json!({}),
                diagnostics::present_selection_error(&error, localizer),
            )
            .emit(args.format);
        }
        Err(error) => return response::source(&error, start, localizer).emit(args.format),
    };
    if matches!(args.format, OutputFormat::Json) {
        let projects: Vec<_> = sources
            .iter()
            .map(|source| project_document(source.project()))
            .collect();
        println!(
            "{}",
            json!({"schema_version":1,"kind":"project-validation","status":"valid","validation":"root_descriptors","projects":projects})
        );
    } else {
        for source in &sources {
            println!(
                "{}",
                response::path_message(localizer, "project-validated", source.project().root())
            );
        }
    }
    ExitCode::SUCCESS
}

/// Create a project with no implicit Git repository, workflow selection or platform discovery.
pub fn new(start: &Path, args: &NewArgs, localizer: &Localizer) -> ExitCode {
    let project_type = match args.project_type.as_str() {
        "configuration" => ProjectType::Configuration,
        "extension" => ProjectType::Extension,
        "processing" => ProjectType::Processing,
        "report" => ProjectType::Report,
        _ => {
            return Failure::new(
                "project_type",
                json!({"value":args.project_type}),
                localizer.text("new-type-invalid"),
            )
            .emit(args.format);
        }
    };
    let manifest = cli::command_path(start, &args.path).join(FILE_NAME);
    let config = match ProjectConfig::new(project_type).with_source(args.source.clone()) {
        Ok(config) => config,
        Err(error) => return response::config(error, &manifest, localizer).emit(args.format),
    };
    let config = match build_settings(args.platform_version.as_deref(), localizer) {
        Ok(Some(settings)) => config.with_build_settings(settings),
        Ok(None) => config,
        Err(error) => return error.emit(args.format),
    };
    let workspace = match onboarding::find_workspace(start) {
        Ok(workspace) => workspace,
        Err(error) => return response::discovery(&error, localizer).emit(args.format),
    };
    let result = if let Some(workspace) = &workspace {
        let name = match member_name(args.path.to_str(), localizer) {
            Ok(name) => name,
            Err(error) => return error.emit(args.format),
        };
        create::inspect_workspace_member::<Infallible>(workspace, name)
            .and_then(|plan| create::create_member_configured(&plan, &config, None))
    } else {
        create::create(&cli::command_path(start, &args.path), &config)
    };
    match result {
        Ok(project) => success(
            "project-new",
            "created",
            "new-created",
            &project,
            args.format,
            localizer,
        ),
        Err(error) => response::creation(error, localizer).emit(args.format),
    }
}

/// Attach an existing export without rewriting XML, payloads or existing control files.
pub fn init(start: &Path, args: &InitArgs, localizer: &Localizer) -> ExitCode {
    let root = cli::command_path(start, &args.path);
    let plan = match init::inspect::<Infallible>(&root, args.source.as_deref()) {
        Ok(plan) => plan,
        Err(error) => return response::initialization(error, localizer).emit(args.format),
    };
    let settings = match build_settings(args.platform_version.as_deref(), localizer) {
        Ok(settings) => settings,
        Err(error) => return error.emit(args.format),
    };
    let workspace = match onboarding::find_workspace(plan.root()) {
        Ok(workspace) => workspace,
        Err(error) => return response::discovery(&error, localizer).emit(args.format),
    };
    let result = if let Some(workspace) = &workspace {
        let name = match member_name(
            args.name
                .as_deref()
                .or_else(|| plan.root().file_name().and_then(|name| name.to_str())),
            localizer,
        ) {
            Ok(name) => name,
            Err(error) => return error.emit(args.format),
        };
        init::prepare_workspace_member::<Infallible>(plan, workspace, name).and_then(|plan| {
            let plan = match settings {
                Some(settings) => plan.with_build_settings(settings),
                None => plan,
            };
            init::apply_workspace_member(&plan)
        })
    } else {
        if args.name.is_some() {
            return Failure::new(
                "workspace_required",
                json!({}),
                localizer.text("init-name-workspace-only"),
            )
            .emit(args.format);
        }
        let config = match ProjectConfig::new(plan.project_type()).with_source(plan.source().into())
        {
            Ok(config) => config,
            Err(error) => {
                return response::config(error, &root.join(FILE_NAME), localizer).emit(args.format);
            }
        };
        let config = match settings {
            Some(settings) => config.with_build_settings(settings),
            None => config,
        };
        init::apply(&plan, &config)
    };
    match result {
        Ok(project) => success(
            "project-init",
            "initialized",
            "init-created",
            &project,
            args.format,
            localizer,
        ),
        Err(error) => response::initialization(error, localizer).emit(args.format),
    }
}

/// Validate the optional data value without locating installed platforms or choosing a fallback.
fn build_settings(
    value: Option<&str>,
    localizer: &Localizer,
) -> Result<Option<BuildSettings>, Failure> {
    value
        .map(|value| {
            BuildSettings::new(value, PathBuf::from(DEFAULT_ARTIFACTS_DIRECTORY)).map_err(|_| {
                Failure::new(
                    "platform_version",
                    json!({"value":value}),
                    localizer.format(
                        "build-platform-version-invalid",
                        &[("value", LocalizationValue::Text(value))],
                    ),
                )
            })
        })
        .transpose()
}

/// Workspace identities remain portable even when ordinary standalone directories contain Unicode.
fn member_name(value: Option<&str>, localizer: &Localizer) -> Result<ProjectName, Failure> {
    let value = value.unwrap_or_default();
    ProjectName::parse(value.into()).map_err(|_| {
        Failure::new(
            "member_name",
            json!({"value":value}),
            localizer.format(
                "new-member-name-invalid",
                &[("name", LocalizationValue::Text(value))],
            ),
        )
    })
}

/// Use the common reversible path representation for every successful project operation.
fn project_document(project: &Project) -> Value {
    json!({"root":response::path(project.root()),"source":response::path(project.source()),"project_type":project.configuration().project_type().as_str()})
}

/// Keep successful machine results independent of locale and avoid implying repository creation.
fn success(
    kind: &str,
    status: &str,
    key: &str,
    project: &Project,
    format: OutputFormat,
    localizer: &Localizer,
) -> ExitCode {
    if matches!(format, OutputFormat::Json) {
        println!(
            "{}",
            json!({"schema_version":1,"kind":kind,"status":status,"project":project_document(project),"vcs_initialized":false})
        );
    } else {
        println!(
            "{}",
            localizer.format(
                key,
                &[(
                    "path",
                    LocalizationValue::Text(&path_output::display_path(project.root()))
                )]
            )
        );
    }
    ExitCode::SUCCESS
}
