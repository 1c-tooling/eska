//! Application context output contains only checked metadata and declared relationships.

use std::{path::Path, process::ExitCode};

use eska_presentation::{
    diagnostics,
    localization::{LocalizationValue, Localizer},
    path_output,
};
use eska_project_core::application::{self, AnalysisContext, ContextError, SourceRole};
use serde_json::{Value, json};

use crate::{
    cli::{ContextArgs, OutputFormat},
    response::{self, Failure},
};

/// Resolve the selected application without scanning BSL modules or starting an analyzer.
pub fn run(start: &Path, args: &ContextArgs, localizer: &Localizer) -> ExitCode {
    let context = match application::resolve(start, args.application.as_deref()) {
        Ok(context) => context,
        Err(error) => return failure(&error, start, localizer).emit(args.format),
    };
    if matches!(args.format, OutputFormat::Json) {
        println!("{}", document(&context));
    } else {
        println!(
            "{}",
            response::path_message(localizer, "project-context-ready", context.root())
        );
        if context.limited_context() {
            println!("{}", localizer.text("project-context-limited"));
        }
        for source in context.sources() {
            println!(
                "{}: {}",
                localizer.text(&format!(
                    "new-type-{}",
                    source.project().configuration().project_type().as_str()
                )),
                path_output::display_path(source.project().source())
            );
        }
    }
    ExitCode::SUCCESS
}

/// Keep paths reversible and machine keys independent of the selected presentation language.
pub fn document(context: &AnalysisContext) -> Value {
    let sources: Vec<_> = context.sources().iter().map(|source| json!({
        "member":source.member(),
        "role":match source.role() {SourceRole::Configuration=>"configuration", SourceRole::Extension=>"extension", SourceRole::ExternalObject=>"external_object"},
        "project_type":source.project().configuration().project_type().as_str(),
        "root":response::path(source.project().root()),
        "source":response::path(source.project().source()),
        "descriptor":response::path(source.descriptor()),
        "metadata_name":source.metadata_name(),
        "uuid":source.uuid(),
        "compatibility_mode":source.compatibility_mode()
    })).collect();
    json!({"schema_version":1,"kind":"project-context","status":"ready","context":{
        "name":context.name(),"root":response::path(context.root()),"limited_context":context.limited_context(),
        "target_platform_version":context.target_platform_version(),"sources":sources,
        "extension_dependencies":context.extension_dependencies()
    }})
}

/// Preserve ambiguity and invalid context as failures, including valid standalone limited contexts.
pub fn failure(error: &ContextError, start: &Path, localizer: &Localizer) -> Failure {
    match error {
        ContextError::Discovery(error) => response::discovery(error, localizer),
        ContextError::Source(error) => response::source(error, start, localizer),
        ContextError::Settings(error) => Failure::new(
            "application_settings",
            json!({}),
            diagnostics::application::present(localizer, &start.join("eska.toml"), error, false),
        ),
        ContextError::UnknownApplication {
            requested,
            available,
        } => Failure::new(
            "unknown_application",
            json!({"requested":requested,"available":available}),
            localizer.format(
                "project-application-unknown",
                &[("name", LocalizationValue::Text(requested))],
            ),
        ),
        ContextError::AmbiguousApplication { available } => Failure::new(
            "ambiguous_application",
            json!({"available":available}),
            localizer.text("project-application-ambiguous"),
        ),
        ContextError::ApplicationDeclarationsMissing { members } => Failure::new(
            "application_declarations_missing",
            json!({"members":members}),
            localizer.text("project-application-required"),
        ),
        ContextError::EmptyWorkspace => Failure::new(
            "no_projects",
            json!({}),
            localizer.text("project-empty-workspace"),
        ),
    }
}
