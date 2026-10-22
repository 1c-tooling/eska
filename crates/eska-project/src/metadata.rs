//! Machine-oriented property inspection and mutation using the same backend as the IDE.

use eska_presentation::arguments::project::metadata::MetadataCommand;
pub use eska_presentation::arguments::project::metadata::{MetadataArgs, localize};

use std::{io::Read, path::Path, process::ExitCode};

mod renaming;

use serde_json::{Value, json};

use crate::ide::{dto, editing};
use eska_presentation::localization::Localizer;
use eska_project_core::project::{
    configurator::TreeOptions,
    metadata_model::{NodeId, ObjectId},
    metadata_workspace::{MetadataWorkspace, ProjectSession},
};

struct MetadataHandler<'a> {
    args: &'a MetadataArgs,
}

impl MetadataHandler<'_> {
    /// Emit exactly one stable JSON envelope; source inspection never starts disk caching.
    #[must_use]
    pub fn run(&self, project_dir: &Path, _localizer: &Localizer) -> ExitCode {
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
        let selected: Vec<_> = self.args.project.iter().cloned().collect();
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
        match &self.args.command {
            MetadataCommand::RenamePreview(args) => {
                let request = read_input(&args.input, &["schemaVersion", "objectId", "newName"])?;
                renaming::preview(project, &request)
            }
            MetadataCommand::RenameApply(args) => {
                let request = read_input(
                    &args.input,
                    &[
                        "schemaVersion",
                        "objectId",
                        "newName",
                        "snapshot",
                        "reviewedUncertain",
                    ],
                )?;
                renaming::apply(project, &request)
            }
            MetadataCommand::RenameRecovery(args) => {
                let request = read_input(&args.input, &["schemaVersion", "action", "snapshot"])?;
                renaming::recovery(project, &request)
            }
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
                let input = read_input(
                    &args.input,
                    if matches!(self.args.command, MetadataCommand::Choices(_)) {
                        &["schemaVersion", "objectId", "path", "key"]
                    } else {
                        &["schemaVersion", "objectId", "path"]
                    },
                )?;
                let id: ObjectId = serde_json::from_value(input["objectId"].clone())
                    .map_err(|_| failure("invalid_request"))?;
                let path: Vec<eska_project_core::project::metadata_edit::FieldStep> =
                    serde_json::from_value(input["path"].clone())
                        .map_err(|_| failure("invalid_request"))?;
                reveal(project, &id)?;
                if matches!(self.args.command, MetadataCommand::Choices(_)) {
                    if let Some(key) = input.get("key") {
                        let key = serde_json::from_value(key.clone())
                            .map_err(|_| failure("invalid_request"))?;
                        editing::value_choices(&labels, project, &id, &path, &key)
                    } else {
                        editing::reference_choices(&labels, project, &id, &path)
                    }
                } else {
                    editing::choices(&labels, project, &id, &path)
                }
            }
            MetadataCommand::Check(args) | MetadataCommand::Apply(args) => {
                let input = read_input(
                    &args.input,
                    &[
                        "schemaVersion",
                        "objectId",
                        "snapshot",
                        "contextSnapshot",
                        "path",
                        "change",
                    ],
                )?;
                let request: editing::ChangeRequest =
                    serde_json::from_value(input).map_err(|_| failure("invalid_request"))?;
                reveal(project, &request.object_id)?;
                if matches!(self.args.command, MetadataCommand::Check(_)) {
                    return editing::preview(project, &request);
                }
                apply(&labels, project, &request)
            }
        }
    }
}

/// Apply the same verified plan exposed by check, returning a fresh editing schema.
fn apply(
    labels: &dto::Labels,
    project: &mut ProjectSession,
    request: &editing::ChangeRequest,
) -> Result<Value, Value> {
    project
        .update_property(
            &request.object_id,
            &request.snapshot,
            request.context_snapshot.as_deref(),
            &request.path,
            &request.change,
        )
        .map_err(editing::failure)?;
    let state = editing::describe(labels, project, &request.object_id)
        .map_err(|_| failure("property_committed_refresh_required"))?;
    Ok(json!({"applied":true,"objectId":request.object_id,"editing":state}))
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

/// Execute parsed metadata requests through the existing project session.
pub fn run(args: &MetadataArgs, directory: &Path, localizer: &Localizer) -> ExitCode {
    MetadataHandler { args }.run(directory, localizer)
}
