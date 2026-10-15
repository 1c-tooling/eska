//! Declaration-only navigation retains the selected source's identities without activating payloads.

use std::collections::BTreeMap;

use eska::project_core::{
    eska_source::EskaSource,
    metadata_model::{MetadataObject, ModuleRole, ProjectScope},
    source_format::{Error, PropertyName, PropertyValue, SourcePath, SourceReader},
};
use serde_json::{Value, json};

struct Owner {
    declaration: MetadataObject,
    modules: Vec<Value>,
    queries: Vec<Value>,
}

/// Compact declarations survive transfer of the authoritative reader into an analysis session.
pub struct Navigation {
    project: Value,
    owners: BTreeMap<SourcePath, Owner>,
}

impl Navigation {
    /// Capture only validated declarations; no owner, module, query or accepted-base file is opened.
    ///
    /// # Errors
    /// Returns an inconsistent declaration, absent logical name or unrepresentable directory URI.
    pub fn selected(source: &EskaSource) -> Result<Self, Error> {
        let reader = source.reader();
        let root = root_declaration(reader)?;
        let mut owners = BTreeMap::new();
        for path in reader.owner_paths() {
            let declaration = if path == &reader.configuration().path {
                root.clone()
            } else {
                let declared = reader.declared_owner(path)?;
                MetadataObject::new(
                    declared.kind,
                    declared
                        .name
                        .ok_or_else(|| failure("source_name_missing", Some(path)))?
                        .trim()
                        .to_owned(),
                    declared.uuid.as_str().to_owned(),
                    declared.parent.cloned(),
                )
                .map_err(|_| failure("source_name_missing", Some(path)))?
            };
            owners.insert(
                path.clone(),
                Owner {
                    declaration,
                    modules: Vec::new(),
                    queries: Vec::new(),
                },
            );
        }
        for path in reader.module_paths() {
            let module = reader
                .declared_module(path)
                .ok_or_else(|| failure("source_module_undeclared", Some(path)))?;
            let owner = owners
                .get_mut(module.owner)
                .ok_or_else(|| failure("source_identity_mismatch", Some(path)))?;
            owner
                .modules
                .push(json!({"source":path,"native_path":module.native_path,
                "native_descriptor":module.native_descriptor,
                "role":module_role(module.native_path).as_str(),"binary":module.binary}));
        }
        let resource_sources = reader
            .manifest()
            .files
            .iter()
            .map(|file| (&file.path, file.source_path()))
            .collect::<BTreeMap<_, _>>();
        for path in reader.query_paths() {
            let query = reader
                .declared_query(path)
                .ok_or_else(|| failure("source_query_undeclared", Some(path)))?;
            let owner = owners
                .get_mut(query.owner)
                .ok_or_else(|| failure("source_identity_mismatch", Some(path)))?;
            let resource_source = resource_sources
                .get(query.native_resource)
                .ok_or_else(|| failure("source_query_undeclared", Some(path)))?;
            owner.queries.push(
                json!({"source":path,"native_resource":query.native_resource,
                    "resource_source":resource_source}),
            );
        }
        let scope = match source.scope() {
            ProjectScope::Standalone => json!({"kind":"standalone"}),
            ProjectScope::Member(name) => json!({"kind":"member","name":name.as_str()}),
        };
        let root_uri = url::Url::from_directory_path(source.project().root())
            .map_err(|()| failure("path_encoding_unsupported", None))?;
        let source_uri = url::Url::from_directory_path(reader.root())
            .map_err(|()| failure("path_encoding_unsupported", None))?;
        Ok(Self {
            project: json!({"project_type":source.project().configuration().project_type().as_str(),
                "scope":scope,"root_uri":root_uri.as_str(),"source_uri":source_uri.as_str(),
                "root_source":reader.configuration().path}),
            owners,
        })
    }

    /// Return the selected project context without readiness, permission or kernel claims.
    #[must_use]
    pub fn project(&self) -> Value {
        self.project.clone()
    }

    /// Return one declaration and its explicit source bindings without reading current payload bytes.
    #[must_use]
    pub fn owner(&self, source: &SourcePath) -> Option<Value> {
        let owner = self.owners.get(source)?;
        let object = &owner.declaration;
        Some(
            json!({"declaration":{"id":object.id(),"kind":object.kind().as_str(),
            "name":object.name(),"uuid":object.uuid(),"parent":object.parent()},
            "modules":owner.modules,"queries":owner.queries}),
        )
    }
}

/// Root profiles retain their actual metadata kind; external artifacts never become Configurations.
fn root_declaration(reader: &SourceReader) -> Result<MetadataObject, Error> {
    let root = &reader.configuration().source.document.descriptor.object;
    let Some(PropertyValue::Text(name)) = root.property(PropertyName::Name) else {
        return Err(failure(
            "source_name_missing",
            Some(&reader.configuration().path),
        ));
    };
    MetadataObject::new(
        root.kind,
        name.trim().to_owned(),
        root.uuid.as_str().to_owned(),
        None,
    )
    .map_err(|_| failure("source_name_missing", Some(&reader.configuration().path)))
}

/// Public structured source failures need only a stable code and the exact declaration address.
fn failure(code: &'static str, source: Option<&SourcePath>) -> Error {
    Error {
        code,
        location: source
            .into_iter()
            .map(|path| path.as_str().to_owned())
            .collect(),
    }
}

/// Validated native basenames use existing roles; an unfamiliar future module stays a generic module.
pub fn module_role(native: &SourcePath) -> ModuleRole {
    let stem = native
        .as_path()
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_ascii_lowercase);
    match stem.as_deref() {
        Some("objectmodule") => ModuleRole::Object,
        Some("managermodule") => ModuleRole::Manager,
        Some("recordsetmodule") => ModuleRole::RecordSet,
        Some("valuemanagermodule") => ModuleRole::ValueManager,
        Some("managedapplicationmodule") => ModuleRole::ManagedApplication,
        Some("ordinaryapplicationmodule") => ModuleRole::OrdinaryApplication,
        Some("sessionmodule") => ModuleRole::Session,
        Some("externalconnectionmodule") => ModuleRole::ExternalConnection,
        Some("commandmodule") => ModuleRole::Command,
        _ => ModuleRole::Module,
    }
}
