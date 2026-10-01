//! Read the current declaration graph without relying on the tree or watcher cache.

use std::path::PathBuf;

use super::{PropertyEditError, WorkspaceError};
use crate::project::{
    designer_source::DesignerSource,
    metadata_edit::snapshot,
    metadata_parser::{self, PropertiesMode},
};
use crate::project::{
    metadata_edit::{EditError, PropertyFileEdit, numbering},
    metadata_model::{MetadataKind, ObjectId},
};
use std::fmt::Write;

pub(super) struct ContextFile {
    pub id: ObjectId,
    pub name: String,
    pub path: PathBuf,
    pub input: String,
}

pub(super) struct NumberingContext {
    id: ObjectId,
    pub files: Vec<ContextFile>,
}

impl NumberingContext {
    /// A new document can join the numerator even if no watcher notification has arrived yet.
    pub fn read(
        source: &DesignerSource,
        id: &ObjectId,
        kind: MetadataKind,
    ) -> Result<Self, PropertyEditError> {
        let fresh = source.reopen().map_err(WorkspaceError::Source)?;
        let root = fresh
            .read_xml(fresh.descriptor())
            .map_err(WorkspaceError::Source)?
            .ok_or(EditError::Conflict)?;
        let root = metadata_parser::parse(&root, None, PropertiesMode::Summary)
            .map_err(|_| EditError::InvalidXml)?;
        let owner = root
            .references
            .iter()
            .find(|reference| reference.id == *id && reference.kind == kind)
            .ok_or(EditError::Conflict)?;
        let target = format!("DocumentNumerator.{}", owner.name).to_lowercase();
        let selected = if kind == MetadataKind::Document {
            MetadataKind::DocumentNumerator
        } else {
            MetadataKind::Document
        };
        let mut files = Vec::new();
        for reference in root
            .references
            .iter()
            .filter(|reference| reference.kind == selected && reference.owner == *fresh.root().id())
        {
            let location = fresh
                .object_descriptor(&reference.id)
                .map_err(WorkspaceError::Source)?
                .ok_or(EditError::Conflict)?;
            let input = fresh
                .read_xml(&location.path)
                .map_err(WorkspaceError::Source)?
                .ok_or(EditError::Conflict)?;
            if selected == MetadataKind::Document
                && numbering::document_numerator(&input)?
                    .is_none_or(|value| value.to_lowercase() != target)
            {
                continue;
            }
            let parsed = metadata_parser::parse(&input, None, PropertiesMode::Summary)
                .map_err(|_| EditError::InvalidXml)?;
            if parsed
                .objects
                .first()
                .is_none_or(|object| object.metadata.id() != &reference.id)
            {
                return Err(EditError::Conflict.into());
            }
            files.push(ContextFile {
                id: reference.id.clone(),
                name: reference.name.clone(),
                path: location.path,
                input,
            });
        }
        files.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(Self {
            id: id.clone(),
            files,
        })
    }

    /// Scope and metadata identities prevent reusing a token for another numerator in the same source.
    pub fn snapshot(&self) -> String {
        self.after(std::iter::empty())
    }

    /// Compute the final dependency token from planned bytes before the first file is replaced.
    pub fn after<'a>(&self, edits: impl Iterator<Item = &'a PropertyFileEdit>) -> String {
        let edits: std::collections::BTreeMap<_, _> = edits
            .map(|file| (&file.object_id, file.plan.output()))
            .collect();
        let mut value = format!("metadata-numbering-1\n{}\n", self.id.as_str());
        for file in &self.files {
            let content = edits.get(&file.id).copied().unwrap_or(&file.input);
            let _ = writeln!(
                value,
                "{}:{}:{}",
                file.id.as_str().len(),
                file.id.as_str(),
                snapshot(content)
            );
        }
        snapshot(&value)
    }

    /// Case-insensitive metadata names retain the exact identity verified in the current root declaration.
    pub fn numerator(&self, value: &str) -> Result<&ContextFile, EditError> {
        let parts = crate::project::metadata_edit::references::parts(value)
            .ok_or(EditError::InvalidValue)?;
        let [(MetadataKind::DocumentNumerator, name)] = parts.as_slice() else {
            return Err(EditError::InvalidValue);
        };
        let target = name.to_lowercase();
        let mut matches = self
            .files
            .iter()
            .filter(|file| file.name.to_lowercase() == target);
        let file = matches.next().ok_or(EditError::InvalidValue)?;
        if matches.next().is_some() {
            return Err(EditError::UnsupportedValue);
        }
        Ok(file)
    }
}
