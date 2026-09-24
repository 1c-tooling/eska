//! Client file notifications invalidate existing sessions without owning a watcher.

use super::{
    envelope::{domain, error},
    metadata::changed,
    params,
    server::Server,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Component, Path};

impl Server {
    /// Invalid packets with an identifiable project require recovery instead of disappearing.
    pub(super) fn files_changed(
        &mut self,
        args: &Value,
        events: &mut Vec<Value>,
    ) -> Result<(), Value> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Target {
            session_id: String,
            project_id: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Changes {
            sequence: String,
            paths: Vec<params::Path>,
            manifest_changed: Option<bool>,
        }
        let target: Target = params::decode(args)?;
        let session = self
            .session
            .as_mut()
            .filter(|session| session.id == target.session_id)
            .ok_or_else(|| domain("unknown_session", json!({})))?;
        let index = session
            .projects
            .iter()
            .position(|state| state.id == target.project_id)
            .ok_or_else(|| domain("unknown_project", json!({})))?;
        let input: Result<Changes, _> =
            params::optional_fields(args, &["manifestChanged"]).and_then(|()| params::decode(args));
        if input
            .as_ref()
            .is_ok_and(|input| input.manifest_changed == Some(true))
        {
            for state in &mut session.projects {
                state.reopen = true;
                let project = session
                    .workspace
                    .project(&state.scope)
                    .map_err(|_| error(-32603))?;
                changed(&session.id, state, project, Value::Null, events)?;
            }
            return Ok(());
        }
        let state = &mut session.projects[index];
        let project = session
            .workspace
            .project_mut(&state.scope)
            .map_err(|_| error(-32603))?;
        let result = input.and_then(|input| {
            let sequence = params::counter(&input.sequence)?;
            let expected = state.file.checked_add(1);
            state.file = sequence;
            if expected != Some(sequence) || input.paths.is_empty() || input.paths.len() > 4096 {
                return Err(error(-32602));
            }
            let paths = input
                .paths
                .iter()
                .map(params::Path::native)
                .collect::<Result<Vec<_>, _>>()?;
            if paths
                .iter()
                .any(|path| !contained(project.project().source(), path))
            {
                return Err(error(-32602));
            }
            if state.refresh || state.reopen {
                return Err(error(-32602));
            }
            project
                .changed_paths(&paths)
                .map_err(|failure| super::errors::workspace(&failure))
        });
        if result
            .as_ref()
            .is_ok_and(|report| report.affected.is_empty())
        {
            return Ok(());
        }
        if result.is_err() {
            state.refresh = true;
        }
        let affected = result
            .as_ref()
            .map_or(Value::Null, |report| json!(report.affected));
        changed(&session.id, state, project, affected, events)?;
        Self::progress_event(&session.id, state, project, events);
        result.map(|_| ())
    }

    /// A bounded queue overflow conservatively invalidates every currently open project.
    pub(super) fn file_overflow(&mut self, events: &mut Vec<Value>) {
        if let Some(session) = &mut self.session {
            for state in &mut session.projects {
                state.refresh = true;
                if let Ok(project) = session.workspace.project(&state.scope) {
                    let _ = changed(&session.id, state, project, Value::Null, events);
                }
            }
        }
    }
}

/// Check each existing ancestor so deleted paths cannot conceal a symlink escape.
fn contained(root: &Path, relative: &Path) -> bool {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return false;
    }
    let mut current = root.to_path_buf();
    for part in relative.components() {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(_) => match current.canonicalize() {
                Ok(path) if path.starts_with(root) => (),
                _ => return false,
            },
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return false,
        }
    }
    true
}
