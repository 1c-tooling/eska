//! A fixed control-file inventory prevents queries from silently outliving their opening composition.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use eska::project_core::{config::FILE_NAME, eska_source::EskaSource};
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
                source.reader().root().join("configuration.yaml"),
                Kind::RootConfiguration,
            ),
            (
                source.project().root().join(FILE_NAME),
                Kind::ProjectSettings,
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
