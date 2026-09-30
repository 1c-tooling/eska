//! Machine-oriented property inspection and mutation using the same backend as the IDE.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Args, Subcommand, ValueEnum};
use serde_json::{Value, json};

use crate::{
    cli::{
        ide::{dto, editing},
        localization::Localizer,
    },
    project::{
        configurator::TreeOptions,
        metadata_model::{NodeId, ObjectId},
        metadata_workspace::{MetadataWorkspace, ProjectSession},
    },
};

#[derive(Debug, Args)]
pub(in crate::cli) struct MetadataArgs {
    #[command(subcommand)]
    command: MetadataCommand,
    #[arg(short = 'p', long, global = true)]
    project: Option<String>,
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Json,
}

#[derive(Debug, Subcommand)]
enum MetadataCommand {
    #[command(disable_help_flag = true)]
    Inspect(InspectArgs),
    #[command(disable_help_flag = true)]
    Types(InputArgs),
    #[command(disable_help_flag = true)]
    Choices(InputArgs),
    #[command(disable_help_flag = true)]
    Check(InputArgs),
    #[command(disable_help_flag = true)]
    Apply(InputArgs),
}

#[derive(Debug, Args)]
struct InspectArgs {
    #[arg(long)]
    object: Option<String>,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Args)]
struct InputArgs {
    input: PathBuf,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl MetadataArgs {
    /// Emit exactly one stable JSON envelope; source inspection never starts disk caching.
    pub(super) fn run(&self, project_dir: &Path, _localizer: &Localizer) -> ExitCode {
        let result = self.execute(project_dir);
        match result {
            Ok(result) => {
                println!("{}", json!({"schema_version":1,"result":result}));
                ExitCode::SUCCESS
            }
            Err(error) => {
                println!(
                    "{}",
                    json!({"schema_version":1,"error":error.get("data").unwrap_or(&error)})
                );
                ExitCode::FAILURE
            }
        }
    }

    /// Each process selects exactly one manifest-backed project, including workspace members.
    fn execute(&self, project_dir: &Path) -> Result<Value, Value> {
        let selected: Vec<_> = self.project.iter().cloned().collect();
        let mut workspace = MetadataWorkspace::open(project_dir, &selected, false)
            .map_err(|error| editing::failure(error.into()))?;
        if workspace.projects().len() != 1 {
            return Err(failure("selection_invalid"));
        }
        let scope = workspace.projects()[0].scope().clone();
        let project = workspace
            .project_mut(&scope)
            .map_err(|error| editing::failure(error.into()))?;
        let labels = dto::Labels::new().map_err(|_| failure("localization_failed"))?;
        match &self.command {
            MetadataCommand::Inspect(args) => {
                let id = args.object.as_ref().map_or_else(
                    || match project.root() {
                        NodeId::Object(id) => Ok(id.clone()),
                        _ => Err(failure("unknown_object")),
                    },
                    |id| serde_json::from_value(json!(id)).map_err(|_| failure("invalid_request")),
                )?;
                inspect(&labels, project, &id)
            }
            MetadataCommand::Types(args) | MetadataCommand::Choices(args) => {
                let input = read_input(&args.input, &["schemaVersion", "objectId", "path"])?;
                let id: ObjectId = serde_json::from_value(input["objectId"].clone())
                    .map_err(|_| failure("invalid_request"))?;
                let path: Vec<crate::project::metadata_edit::FieldStep> =
                    serde_json::from_value(input["path"].clone())
                        .map_err(|_| failure("invalid_request"))?;
                reveal(project, &id)?;
                if matches!(self.command, MetadataCommand::Choices(_)) {
                    editing::reference_choices(&labels, project, &id, &path)
                } else {
                    editing::choices(&labels, project, &id, &path)
                }
            }
            MetadataCommand::Check(args) | MetadataCommand::Apply(args) => {
                let input = read_input(
                    &args.input,
                    &["schemaVersion", "objectId", "snapshot", "path", "change"],
                )?;
                let request: editing::ChangeRequest =
                    serde_json::from_value(input).map_err(|_| failure("invalid_request"))?;
                reveal(project, &request.object_id)?;
                if matches!(self.command, MetadataCommand::Check(_)) {
                    return editing::preview(project, &request);
                }
                project
                    .update_property(
                        &request.object_id,
                        &request.snapshot,
                        &request.path,
                        &request.change,
                    )
                    .map_err(editing::failure)?;
                let state = editing::describe(&labels, project, &request.object_id)
                    .map_err(|_| failure("property_committed_refresh_required"))?;
                Ok(json!({"applied":true,"objectId":request.object_id,"editing":state}))
            }
        }
    }
}

/// Resolve only declared ancestors of the supplied ID, including inline attributes and commands.
fn reveal(project: &mut ProjectSession, id: &ObjectId) -> Result<(), Value> {
    project
        .reveal_declared_object(id)
        .map(|_| ())
        .map_err(|error| editing::failure(error.into()))
}

/// Return current values, immutable identities, editable schema and immediate child IDs together.
fn inspect(
    labels: &dto::Labels,
    project: &mut ProjectSession,
    id: &ObjectId,
) -> Result<Value, Value> {
    reveal(project, id)?;
    let summary = project
        .object(id)
        .map_err(|error| editing::failure(error.into()))?
        .clone();
    let properties = project
        .properties(id)
        .map_err(|error| editing::failure(error.into()))?;
    let mut children = Vec::new();
    let mut collections = vec![NodeId::Object(id.clone())];
    while let Some(collection) = collections.pop() {
        let nodes = project
            .children(&collection, TreeOptions::default())
            .map_err(|error| editing::failure(error.into()))?;
        for node in nodes {
            if matches!(node.id, NodeId::Collection { .. }) {
                collections.push(node.id.clone());
            } else {
                children.push(labels.node(node));
            }
        }
    }
    Ok(
        json!({"object":dto::object(&summary),"properties":properties.iter().map(|property| dto::property(labels, summary.kind, property)).collect::<Vec<_>>(),
        "editing":editing::describe(labels, project, id)?,"children":children}),
    )
}

/// Bound input files and stdin identically, rejecting unknown operation envelope members.
fn read_input(path: &Path, allowed: &[&str]) -> Result<Value, Value> {
    let mut bytes = Vec::new();
    let input: Box<dyn Read> = if path == Path::new("-") {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(path).map_err(|_| failure("input_unavailable"))?)
    };
    input
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(|_| failure("input_unavailable"))?;
    if bytes.len() > 1_048_576 {
        return Err(failure("input_too_large"));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| failure("invalid_request"))?;
    if value["schemaVersion"] != 1
        || value
            .as_object()
            .is_none_or(|object| object.keys().any(|key| !allowed.contains(&key.as_str())))
    {
        return Err(failure("invalid_request"));
    }
    Ok(value)
}

/// CLI-only input failures share the stable code/details shape of domain errors.
fn failure(code: &str) -> Value {
    json!({"kind":code,"details":{}})
}

/// Localize help while keeping every machine response independent of --lang.
pub(super) fn localize(mut command: clap::Command, locale: &Localizer) -> clap::Command {
    command = command
        .about(locale.text("metadata-about"))
        .mut_arg("project", |arg| {
            arg.help(locale.text("metadata-project-help"))
        })
        .mut_arg("format", |arg| {
            arg.help(locale.text("metadata-format-help"))
        })
        .mut_arg("help", |arg| arg.help(locale.text("cli-help")));
    for name in ["inspect", "types", "choices", "check", "apply"] {
        command = command.mut_subcommand(name, |command| {
            let command = command
                .about(locale.text(&format!("metadata-{name}-about")))
                .mut_arg("help", |arg| arg.help(locale.text("cli-help")));
            if name == "inspect" {
                command.mut_arg("object", |arg| {
                    arg.help(locale.text("metadata-object-help"))
                })
            } else {
                command.mut_arg("input", |arg| arg.help(locale.text("metadata-input-help")))
            }
        });
    }
    command
}
