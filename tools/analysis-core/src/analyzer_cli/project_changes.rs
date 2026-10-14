//! A fixed control-file inventory prevents queries from silently outliving their opening composition.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use eska::project_core::{
    config::FILE_NAME, eska_source::EskaSource, source_format::PROJECT_SOURCE_IMPORT_RECEIPT,
};
use serde::Serialize;
use serde_json::{Value, json};

use super::{protocol, watched_files};

pub const EVENT: &str = "eska/restartRequired";

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    SourceManifest,
    RootConfiguration,
    ProjectSettings,
    WorkspaceSettings,
    AnalysisSettings,
    SourceImport,
}

#[derive(Serialize)]
pub struct Change {
    pub kind: Kind,
    pub uri: String,
}

pub struct Inputs(BTreeMap<PathBuf, Kind>);

impl Inputs {
    /// Retain only exact paths actually defining this selected session, including absent config candidates.
    pub fn selected(source: &EskaSource, workspace: Option<&Path>) -> Self {
        let mut inputs = BTreeMap::from([
            (
                source.reader().root().join("source.json"),
                Kind::SourceManifest,
            ),
            (
                source
                    .reader()
                    .root()
                    .join(source.reader().configuration().path.as_path()),
                Kind::RootConfiguration,
            ),
            (
                source.project().root().join(FILE_NAME),
                Kind::ProjectSettings,
            ),
            (
                source.project().root().join(PROJECT_SOURCE_IMPORT_RECEIPT),
                Kind::SourceImport,
            ),
        ]);
        for name in project_model::CONFIG_FILE_NAMES {
            inputs.insert(source.project().root().join(name), Kind::AnalysisSettings);
        }
        if let Some(root) = workspace {
            inputs.insert(root.join(FILE_NAME), Kind::WorkspaceSettings);
        }
        Self(inputs)
    }

    /// File event classification is lexical and never reads a changed manifest or settings file.
    pub fn kind(&self, path: &Path) -> Option<Kind> {
        self.0.get(path).copied()
    }

    /// A live import retires old inputs even when its watcher notification has not arrived yet.
    pub fn active_import(&self) -> Result<Option<Change>, Value> {
        let Some((path, kind)) = self
            .0
            .iter()
            .find(|(_, kind)| matches!(kind, Kind::SourceImport))
        else {
            return Ok(None);
        };
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => {
                return Err(protocol::error_value(
                    -32000,
                    "project_input_unavailable",
                    Value::Null,
                ));
            }
            Ok(_) => {}
        }
        let uri = url::Url::from_file_path(path).map_err(|()| {
            protocol::error_value(-32603, "path_encoding_unsupported", Value::Null)
        })?;
        Ok(Some(Change {
            kind: *kind,
            uri: uri.into(),
        }))
    }

    /// Initialization rechecks this boundary after allocating the host and before advertising it.
    pub fn require_idle(&self) -> Result<(), Value> {
        if self.active_import()?.is_some() {
            return Err(protocol::error_value(
                -32000,
                "source_import_incomplete",
                Value::Null,
            ));
        }
        Ok(())
    }

    /// Relative literal filenames keep glob syntax out of arbitrary project and workspace paths.
    pub fn watchers(&self) -> Result<Vec<Value>, Value> {
        self.0
            .keys()
            .map(|path| {
                let invalid =
                    || protocol::error_value(-32603, "path_encoding_unsupported", Value::Null);
                let parent = path.parent().ok_or_else(invalid)?;
                let base = url::Url::from_directory_path(parent).map_err(|()| invalid())?;
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(invalid)?;
                Ok(json!({"globPattern":{"baseUri":base.as_str(),"pattern":name},"kind":7}))
            })
            .collect()
    }

    /// Clients watch this explicit small inventory rather than guessing project or workspace boundaries.
    pub fn capability(&self) -> Result<Value, Value> {
        let inputs = self
            .0
            .iter()
            .map(|(path, kind)| {
                let uri = url::Url::from_file_path(path).map_err(|()| {
                    protocol::error_value(-32603, "path_encoding_unsupported", Value::Null)
                })?;
                Ok(Change {
                    kind: *kind,
                    uri: uri.into(),
                })
            })
            .collect::<Result<Vec<_>, Value>>()?;
        Ok(
            json!({"method":watched_files::METHOD,"policy":"restart","notification":EVENT,"inputs":inputs}),
        )
    }
}

/// All operations after a control change share one stable error and the last accepted snapshot.
pub fn error(state: &Value) -> Value {
    protocol::error_value(-32000, "project_restart_required", state.clone())
}
