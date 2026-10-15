//! Read-only source protection uses an independent accepted pin, not semantic admission.

use std::{collections::BTreeMap, path::PathBuf};

use eska::{
    analysis_core::InputState,
    project_core::{
        Project,
        eska_source::EskaSource,
        metadata_model::ProjectScope,
        source_format::{AcceptedRef, Error, NativeUuid, Operation, Permission, SourcePath},
    },
};
use serde_json::{Value, json};

use crate::{AnalysisSession, RequestCancellation};

enum Target {
    Metadata,
    Module { owner: SourcePath },
    Resource { owner: SourcePath, uuid: NativeUuid },
}

/// Captured declarations locate the selected project; only a caller-supplied pin approves policy.
pub struct ReadPermissions {
    project: Project,
    scope: ProjectScope,
    reference: Option<AcceptedRef>,
    targets: BTreeMap<SourcePath, Target>,
    source_root: PathBuf,
    manifest_sha256: String,
    root_sha256: String,
}

impl ReadPermissions {
    /// Capture the same validated source map as navigation without opening accepted storage.
    pub fn selected(source: &EskaSource, reference: Option<AcceptedRef>) -> Self {
        let reader = source.reader();
        let mut targets = reader
            .owner_paths()
            .map(|path| (path.clone(), Target::Metadata))
            .collect::<BTreeMap<_, _>>();
        for path in reader.module_paths() {
            if let Some(module) = reader.declared_module(path) {
                targets.insert(
                    path.clone(),
                    Target::Module {
                        owner: module.owner.clone(),
                    },
                );
            }
        }
        for path in reader.query_paths() {
            if let Some(query) = reader.declared_query(path) {
                targets.insert(
                    path.clone(),
                    Target::Resource {
                        owner: query.owner.clone(),
                        uuid: query.uuid.clone(),
                    },
                );
            }
        }
        Self {
            project: source.project().clone(),
            scope: source.scope().clone(),
            reference,
            targets,
            source_root: reader.root().to_owned(),
            manifest_sha256: reader.configuration().manifest_sha256.clone(),
            root_sha256: reader.configuration().source_sha256.clone(),
        }
    }

    /// Return current permissions without enabling edits, loading kernel models or advancing source.
    ///
    /// Pending/refused current metadata and absent approval remain unknown and immutable.
    /// # Errors
    /// Returns undeclared source, changed composition, import guard, invalid pin or cancellation.
    pub fn files(
        &self,
        session: &AnalysisSession,
        sources: &[SourcePath],
        cancellation: &RequestCancellation,
    ) -> Result<Value, Error> {
        if session.root() != self.source_root {
            return Err(failure("source_identity_mismatch", None));
        }
        for source in sources {
            if !self.targets.contains_key(source) {
                return Err(failure("source_edit_undeclared", Some(source)));
            }
        }
        if cancellation.is_cancelled() {
            return Err(failure("source_read_cancelled", None));
        }
        let Some(reference) = &self.reference else {
            return Ok(json!(sources.iter().map(unknown).collect::<Vec<_>>()));
        };
        let source = EskaSource::open(self.project.clone(), self.scope.clone())?;
        if source.reader().configuration().manifest_sha256 != self.manifest_sha256
            || source.reader().configuration().source_sha256 != self.root_sha256
        {
            return Err(failure("source_input_changed", None));
        }
        let base = source.edit_authority_cancellable(reference, &|| cancellation.is_cancelled())?;
        let mut files = Vec::with_capacity(sources.len());
        for source in sources {
            if cancellation.is_cancelled() {
                return Err(failure("source_read_cancelled", None));
            }
            let Some(target) = self.targets.get(source) else {
                return Err(failure("source_edit_undeclared", Some(source)));
            };
            let permissions = match target {
                Target::Metadata => {
                    let current = session
                        .metadata(source, session.revision())
                        .map_err(|_| failure("source_edit_undeclared", Some(source)))?;
                    let InputState::Ready(input) = current.input else {
                        files.push(unknown(source));
                        continue;
                    };
                    input
                        .documents
                        .iter()
                        .flat_map(|document| &document.descriptor.objects)
                        .map(|object| {
                            NativeUuid::parse(object.metadata.uuid())
                                .map(|uuid| base.permission(&uuid, Operation::Resource))
                        })
                        .collect::<Result<Vec<_>, _>>()?
                }
                Target::Module { owner } => {
                    if owner_unavailable(session, owner) {
                        files.push(unknown(source));
                        continue;
                    }
                    vec![base.module_permission(source)?]
                }
                Target::Resource { owner, uuid } => {
                    if owner_unavailable(session, owner) {
                        files.push(unknown(source));
                        continue;
                    }
                    vec![base.permission(uuid, Operation::Resource)]
                }
            };
            files.push(file(source, &permissions));
        }
        Ok(json!(files))
    }
}

/// Unread or failed owners must not recover permissions by substituting accepted/saved identities.
fn owner_unavailable(session: &AnalysisSession, source: &SourcePath) -> bool {
    session
        .metadata(source, session.revision())
        .map_or(true, |view| !matches!(view.input, InputState::Ready(_)))
}

/// A physical YAML file is immutable when any owned inline object denies the operation.
fn file(source: &SourcePath, permissions: &[Permission]) -> Value {
    let unknown = permissions.is_empty()
        || permissions
            .iter()
            .any(|permission| permission.state == eska::project_core::support::State::Unknown);
    json!({"source":source,"read_only":unknown || permissions.iter().any(|permission| !permission.allowed),
        "unknown":unknown,"objects":permissions})
}

/// Missing approval is distinct from native support removal or an editable own object.
fn unknown(source: &SourcePath) -> Value {
    json!({"source":source,"read_only":true,"unknown":true,"objects":[]})
}

/// The presentation boundary retains only stable error codes and declared source coordinates.
fn failure(code: &'static str, source: Option<&SourcePath>) -> Error {
    Error {
        code,
        location: source
            .into_iter()
            .map(|source| source.as_str().to_owned())
            .collect(),
    }
}
