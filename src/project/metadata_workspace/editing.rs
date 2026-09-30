//! Property mutations share the checked source resolver, support rules and invalidation boundary.

use std::path::PathBuf;

use super::{ProjectSession, RefreshReport, WorkspaceError};
use crate::project::{
    designer_source::SourceLocation,
    metadata_edit::{
        EditError, EditPlan, EditingDocument, FieldStep, PropertyChange, PropertyEditing, SavedEdit,
    },
    metadata_model::{MetadataKind, ObjectId},
    support::State,
};

/// Source mapping accompanies a fresh editable snapshot, never client-provided paths.
#[derive(Debug)]
pub struct EditingSnapshot {
    pub properties: PropertyEditing,
    pub path: PathBuf,
    pub writable: bool,
    pub undo: bool,
    pub redo: bool,
}

/// A type choice carries semantic identity and optional metadata context for presentation.
#[derive(Debug)]
pub struct PropertyTypeChoice {
    pub key: crate::project::metadata_model::PropertyKey,
    pub object: Option<super::ObjectSummary>,
}

/// Source failures and editing failures retain their own stable machine categories.
#[derive(Debug)]
pub enum PropertyEditError {
    Workspace(Box<WorkspaceError>),
    /// The bytes were published, but refreshing the session failed; do not repeat the write.
    Committed(Box<WorkspaceError>),
    Edit(EditError),
}

impl From<WorkspaceError> for PropertyEditError {
    /// Preserve original source and generation diagnostics at the editing boundary.
    fn from(value: WorkspaceError) -> Self {
        Self::Workspace(Box::new(value))
    }
}

impl From<EditError> for PropertyEditError {
    /// Keep mutation diagnostics independent of localization and transport.
    fn from(value: EditError) -> Self {
        Self::Edit(value)
    }
}

/// One object's history is retained only within the current backend session.
#[derive(Debug, Default)]
pub(super) struct PropertyHistory {
    undo: Vec<SavedEdit>,
    redo: Vec<SavedEdit>,
}

impl ProjectSession {
    /// Read allowed existing data types without indexing or reading unrelated descriptors.
    ///
    /// # Errors
    /// Rejects stale object addresses and fields which do not support type selection.
    pub fn property_type_choices(
        &mut self,
        id: &ObjectId,
        path: &[FieldStep],
    ) -> Result<Vec<PropertyTypeChoice>, PropertyEditError> {
        use crate::project::metadata_edit::{ScalarSchema, types};
        use crate::project::metadata_model::PropertyKey;
        let snapshot = self.property_editing(id)?;
        let field = snapshot
            .properties
            .fields
            .iter()
            .find(|field| field.path == path)
            .ok_or(EditError::UnsupportedValue)?;
        let ScalarSchema::DataType { reference_only, .. } = field.schema else {
            return Err(EditError::UnsupportedValue.into());
        };
        let mut choices: Vec<_> = types::PRIMITIVES
            .iter()
            .filter(|_| !reference_only)
            .map(|(namespace, name)| PropertyTypeChoice {
                key: PropertyKey {
                    namespace: Some((*namespace).to_owned()),
                    name: (*name).to_owned(),
                },
                object: None,
            })
            .collect();
        let sibling_types: Vec<_> = snapshot
            .properties
            .fields
            .iter()
            .filter_map(|other| {
                if other.path == path
                    || other.path.len() != path.len()
                    || other.path[..other.path.len() - 1] != path[..path.len() - 1]
                {
                    return None;
                }
                if let ScalarSchema::DataType { key, .. } = &other.schema {
                    Some(key)
                } else {
                    None
                }
            })
            .collect();
        let root = self.source.root().id();
        for (prefix, tag) in types::REFERENCE_TYPES {
            let kind = MetadataKind::from_xml_tag(tag).map_err(|_| EditError::UnsupportedValue)?;
            for object in self.objects.values().filter(|object| {
                object.kind == kind && object.parent.is_none() && &object.id != root
            }) {
                choices.push(PropertyTypeChoice {
                    key: PropertyKey {
                        namespace: Some(types::CFG.to_owned()),
                        name: format!("{prefix}.{}", object.name),
                    },
                    object: Some(object.clone()),
                });
            }
        }
        choices.retain(|choice| !sibling_types.contains(&&choice.key));
        Ok(choices)
    }

    /// Validate and preview using precisely the same planner as mutation, with no writes.
    ///
    /// # Errors
    /// Returns the same validation, source and policy failures as applying the edit.
    pub fn preview_property(
        &mut self,
        id: &ObjectId,
        expected: &str,
        path: &[FieldStep],
        change: &PropertyChange,
    ) -> Result<EditPlan, PropertyEditError> {
        self.reveal_declared_object(id)?;
        let (location, input) = self.editing_source(id)?;
        if !self.can_edit(id, &location.path)? {
            return Err(EditError::ReadOnly.into());
        }
        if let PropertyChange::DataType { key } = change {
            self.validate_property_type(key)?;
        }
        self.validate_property_reference(id, path, change)?;
        Ok(self
            .editing_document(id, &location, &input)?
            .update(expected, path, change)?)
    }

    /// Read editing domains and the exact source fingerprint without changing XML.
    ///
    /// # Errors
    /// Returns unknown object, invalid source or unsupported descriptor shape.
    pub fn property_editing(
        &mut self,
        id: &ObjectId,
    ) -> Result<EditingSnapshot, PropertyEditError> {
        self.reveal_declared_object(id)?;
        let (location, input) = self.editing_source(id)?;
        let properties = self.editing_document(id, &location, &input)?.inspect()?;
        let writable = self.can_edit(id, &location.path)?
            && std::fs::metadata(self.project().source().join(&location.path))
                .is_ok_and(|metadata| !metadata.permissions().readonly());
        let history = self.property_history.get(id);
        Ok(EditingSnapshot {
            properties,
            path: location.path,
            writable,
            undo: history.is_some_and(|history| !history.undo.is_empty()),
            redo: history.is_some_and(|history| !history.redo.is_empty()),
        })
    }

    /// Validate and publish a single existing scalar, then invalidate its owning descriptor.
    ///
    /// # Errors
    /// Rejects stale snapshots, locked objects, unadvertised fields and invalid values.
    pub fn update_property(
        &mut self,
        id: &ObjectId,
        expected: &str,
        path: &[FieldStep],
        change: &PropertyChange,
    ) -> Result<RefreshReport, PropertyEditError> {
        self.reveal_declared_object(id)?;
        let (location, input) = self.editing_source(id)?;
        if let PropertyChange::DataType { key } = change {
            self.validate_property_type(key)?;
        }
        self.validate_property_reference(id, path, change)?;
        let plan = self
            .editing_document(id, &location, &input)?
            .update(expected, path, change)?;
        let history = plan.history();
        let changed = !plan.is_empty();
        let report = self.publish_property(id, &location, &plan);
        if changed && (report.is_ok() || matches!(report, Err(PropertyEditError::Committed(_)))) {
            let entry = self.property_history.entry(id.clone()).or_default();
            entry.redo.clear();
            entry.undo.push(history);
            while entry.undo.len() > 100
                || entry.undo.iter().map(SavedEdit::bytes).sum::<usize>() > 8 * 1024 * 1024
            {
                entry.undo.remove(0);
            }
            while self.property_history.len() > 256
                || self
                    .property_history
                    .values()
                    .flat_map(|history| history.undo.iter().chain(&history.redo))
                    .map(SavedEdit::bytes)
                    .sum::<usize>()
                    > 32 * 1024 * 1024
            {
                self.property_history.pop_first();
            }
        }
        report
    }

    /// Replay the previous backend-generated replacement without accepting arbitrary XML.
    ///
    /// # Errors
    /// Rejects stale source, changed support policy and unavailable session history.
    pub fn undo_property(
        &mut self,
        id: &ObjectId,
        expected: &str,
        undo: bool,
    ) -> Result<RefreshReport, PropertyEditError> {
        self.reveal_declared_object(id)?;
        let (location, input) = self.editing_source(id)?;
        if crate::project::metadata_edit::snapshot(&input) != expected {
            return Err(EditError::Conflict.into());
        }
        let history = self
            .property_history
            .get(id)
            .ok_or(EditError::HistoryUnavailable)?;
        let step = if undo {
            history.undo.last()
        } else {
            history.redo.last()
        }
        .ok_or(EditError::HistoryUnavailable)?
        .clone();
        let plan = step.replay(&input, undo)?;
        let report = self.publish_property(id, &location, &plan);
        if (report.is_ok() || matches!(report, Err(PropertyEditError::Committed(_))))
            && let Some(history) = self.property_history.get_mut(id)
        {
            let (from, to) = if undo {
                (&mut history.undo, &mut history.redo)
            } else {
                (&mut history.redo, &mut history.undo)
            };
            from.pop();
            to.push(step);
        }
        report
    }

    /// Resolve the existing descriptor again at every write boundary.
    fn editing_source(&self, id: &ObjectId) -> Result<(SourceLocation, String), PropertyEditError> {
        let location = self
            .source
            .object_descriptor(id)
            .map_err(WorkspaceError::Source)?
            .ok_or(EditError::UnsupportedValue)?;
        let input = self
            .source
            .read_xml(&location.path)
            .map_err(WorkspaceError::Source)?
            .ok_or(EditError::UnsupportedValue)?;
        Ok((location, input))
    }

    /// Determine the owning descriptor identity through the existing inline mapping.
    fn editing_document<'a>(
        &self,
        id: &'a ObjectId,
        location: &SourceLocation,
        input: &'a str,
    ) -> Result<EditingDocument<'a>, EditError> {
        let mut owner = id.clone();
        for _ in &location.inline {
            owner = owner.parent().ok_or(EditError::UnsupportedValue)?;
        }
        let predefined = location
            .inline
            .first()
            .is_some_and(|item| item.kind == MetadataKind::PredefinedItem);
        let modern = self
            .project()
            .configuration()
            .build_settings()
            .platform_version()
            .is_some_and(|version| version.as_str().starts_with("8.5.1."));
        Ok(EditingDocument {
            input,
            id,
            parent: if predefined {
                Some(owner)
            } else {
                owner.parent()
            },
            predefined,
            modern,
        })
    }

    /// Object-level support permits a narrow edit even when a sibling in the same XML is locked.
    fn can_edit(&mut self, id: &ObjectId, path: &std::path::Path) -> Result<bool, WorkspaceError> {
        let policy = self.support_files(&[path.to_path_buf()])?;
        Ok(policy.diagnostics.is_empty()
            && policy.objects.iter().any(|object| {
                &object.object_id == id
                    && matches!(
                        object.state,
                        State::Unrestricted | State::EditableWithSupport
                    )
            }))
    }

    /// Generated reference types must name a declared object in this project.
    fn validate_property_type(
        &mut self,
        key: &crate::project::metadata_model::PropertyKey,
    ) -> Result<(), PropertyEditError> {
        use crate::project::metadata_edit::types;
        if key.namespace.as_deref() != Some(types::CFG) {
            return Ok(());
        }
        let (prefix, name) = key.name.split_once('.').ok_or(EditError::InvalidValue)?;
        let tag = types::REFERENCE_TYPES
            .iter()
            .find_map(|(family, tag)| (*family == prefix).then_some(*tag))
            .ok_or(EditError::InvalidValue)?;
        let kind = MetadataKind::from_xml_tag(tag).map_err(|_| EditError::InvalidValue)?;
        self.property_reference(&[(kind, name)])?;
        Ok(())
    }

    /// Publication precedes cache invalidation, and no-op edits leave the generation untouched.
    fn publish_property(
        &mut self,
        id: &ObjectId,
        location: &SourceLocation,
        plan: &EditPlan,
    ) -> Result<RefreshReport, PropertyEditError> {
        if !self.can_edit(id, &location.path)? {
            return Err(EditError::ReadOnly.into());
        }
        if self.generation == u64::MAX {
            return Err(WorkspaceError::GenerationExhausted.into());
        }
        let mut ancestry = Vec::new();
        let mut cursor = Some(id.clone());
        while let Some(ref current) = cursor {
            let object = self.object(current)?;
            ancestry.push((object.kind, object.name.clone()));
            cursor = object.parent.clone();
        }
        ancestry.reverse();
        plan.publish(self.project().source(), &location.path)?;
        if plan.is_empty() {
            return Ok(RefreshReport {
                generation: self.generation,
                affected: Vec::new(),
            });
        }
        let report = self
            .changed_paths(std::slice::from_ref(&location.path))
            .map_err(|error| PropertyEditError::Committed(Box::new(error)))?;
        let parts: Vec<_> = ancestry
            .iter()
            .map(|(kind, name)| (*kind, name.as_str()))
            .collect();
        self.property_reference(&parts)
            .map_err(|error| PropertyEditError::Committed(Box::new(error)))?;
        Ok(report)
    }
}
