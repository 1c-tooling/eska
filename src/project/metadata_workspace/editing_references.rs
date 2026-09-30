//! Resolve selector values against declared objects, without allowing free-form XML references.

use super::{ObjectSummary, ProjectSession, PropertyEditError};
use crate::project::{
    configurator::TreeOptions,
    metadata_edit::{EditError, FieldStep, PropertyChange, ScalarSchema, references},
    metadata_model::{NodeId, ObjectId},
};

/// One allowed Designer reference, with its object identity and human presentation context.
pub struct PropertyReferenceChoice {
    pub value: String,
    pub object: ObjectSummary,
}

impl ProjectSession {
    /// Enumerate references in the property's declared domain, preserving existing list membership.
    ///
    /// # Errors
    /// Rejects unknown objects, fields without a reference domain and invalid source mappings.
    pub fn property_reference_choices(
        &mut self,
        id: &ObjectId,
        path: &[FieldStep],
    ) -> Result<Vec<PropertyReferenceChoice>, PropertyEditError> {
        let editing = self.property_editing(id)?;
        let field = editing
            .properties
            .fields
            .iter()
            .find(|field| field.path == path)
            .ok_or(EditError::UnsupportedValue)?;
        let ScalarSchema::Reference { domain, .. } = &field.schema else {
            return Err(EditError::UnsupportedValue.into());
        };
        let (kind, parent_kind) = references::target(domain).ok_or(EditError::UnsupportedValue)?;
        let owner = self.object(id)?.clone();
        let parent = if let Some(parent_kind) = parent_kind {
            if owner.kind != parent_kind {
                return Err(EditError::UnsupportedValue.into());
            }
            self.children(&NodeId::Object(id.clone()), TreeOptions::default())?;
            Some(id)
        } else {
            None
        };
        let mut result = Vec::new();
        for candidate in self
            .objects
            .values()
            .filter(|object| object.kind == kind && object.parent.as_ref() == parent)
        {
            let value = if parent.is_some() {
                let owner_tag = if &owner.id == self.source.root().id() {
                    match self.project().configuration().project_type() {
                        crate::project::ProjectType::Processing => "ExternalDataProcessor",
                        crate::project::ProjectType::Report => "ExternalReport",
                        _ => owner.kind.designer_tag(),
                    }
                } else {
                    owner.kind.designer_tag()
                };
                format!(
                    "{}.{}.{}.{}",
                    owner_tag,
                    owner.name,
                    kind.designer_tag(),
                    candidate.name
                )
            } else {
                format!("{}.{}", kind.designer_tag(), candidate.name)
            };
            let duplicate = !matches!(field.schema, ScalarSchema::Reference { nullable: true, .. })
                && editing.properties.fields.iter().any(|other| {
                    other.path != field.path
                        && other.path.len() == path.len()
                        && other.path[..path.len() - 1] == path[..path.len() - 1]
                        && other.value == value
                });
            if !duplicate {
                result.push(PropertyReferenceChoice {
                    value,
                    object: candidate.clone(),
                });
            }
        }
        result.sort_by(|left, right| left.value.cmp(&right.value));
        Ok(result)
    }

    /// Both CLI and IDE writes must select an existing object of the exact permitted class and scope.
    pub(super) fn validate_property_reference(
        &mut self,
        id: &ObjectId,
        path: &[FieldStep],
        change: &PropertyChange,
    ) -> Result<(), PropertyEditError> {
        let PropertyChange::Text { value } = change else {
            return Ok(());
        };
        let editing = self.property_editing(id)?;
        let field = editing
            .properties
            .fields
            .iter()
            .find(|field| field.path == path)
            .ok_or(EditError::UnsupportedValue)?;
        if let ScalarSchema::Reference { nullable, .. } = field.schema {
            if value.is_empty() && nullable {
                return Ok(());
            }
            if !self
                .property_reference_choices(id, path)?
                .iter()
                .any(|choice| &choice.value == value)
            {
                return Err(EditError::InvalidValue.into());
            }
            let parts = references::parts(value).ok_or(EditError::InvalidValue)?;
            self.property_reference(&parts)?;
        }
        Ok(())
    }
}
