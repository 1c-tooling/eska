//! Resolve selector values against declared objects, without allowing free-form XML references.

use super::{ObjectSummary, ProjectSession, PropertyEditError};
use crate::project::{
    configurator::TreeOptions,
    metadata_edit::{
        EditError, FieldStep, PropertyChange, ScalarSchema, choice_fields, references, type_links,
        types,
    },
    metadata_model::{MetadataKind, NodeId, ObjectId, PropertyKey},
};

/// One allowed Designer reference, with its object identity and human presentation context.
pub struct PropertyReferenceChoice {
    pub value: String,
    pub object: ObjectSummary,
    pub standard_attribute: Option<String>,
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
        let editing = self.property_fields(id)?;
        let field = editing
            .properties
            .fields
            .iter()
            .find(|field| field.path == path)
            .ok_or(EditError::UnsupportedValue)?;
        let ScalarSchema::Reference {
            domain,
            reference_type,
            ..
        } = &field.schema
        else {
            return Err(EditError::UnsupportedValue.into());
        };
        if matches!(domain.as_str(), choice_fields::DOMAIN | type_links::DOMAIN) {
            return self.property_link_fields(id, path);
        }
        let (kind, parent_kind) = references::target(domain).ok_or(EditError::UnsupportedValue)?;
        let mut owner = self.object(id)?.clone();
        let parent = if domain == "BasicForm" {
            if path
                .last()
                .is_some_and(|step| step.key.name == "ChoiceForm")
            {
                let Some(target) = self.choice_form_owner(reference_type.as_ref())? else {
                    return Ok(Vec::new());
                };
                owner = target;
            } else if !matches!(
                owner.kind,
                MetadataKind::Report | MetadataKind::DataProcessor
            ) {
                return Err(EditError::UnsupportedValue.into());
            }
            self.children(&NodeId::Object(owner.id.clone()), TreeOptions::default())?;
            Some(owner.id.clone())
        } else if let Some(parent_kind) = parent_kind {
            if owner.kind != parent_kind {
                return Err(EditError::UnsupportedValue.into());
            }
            self.children(&NodeId::Object(id.clone()), TreeOptions::default())?;
            Some(id.clone())
        } else {
            None
        };
        let mut result = Vec::new();
        for candidate in self
            .objects
            .values()
            .filter(|object| object.kind == kind && object.parent == parent)
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
                    standard_attribute: None,
                });
            }
        }
        result.sort_by(|left, right| left.value.cmp(&right.value));
        Ok(result)
    }

    /// Follow the field's single generated reference type instead of offering unrelated forms.
    fn choice_form_owner(
        &mut self,
        key: Option<&PropertyKey>,
    ) -> Result<Option<ObjectSummary>, PropertyEditError> {
        let Some(key) = key else {
            return Ok(None);
        };
        let (prefix, name) = key
            .name
            .split_once('.')
            .ok_or(EditError::UnsupportedValue)?;
        let tag = types::REFERENCE_TYPES
            .iter()
            .find_map(|(family, tag)| (*family == prefix).then_some(*tag))
            .ok_or(EditError::UnsupportedValue)?;
        let kind = MetadataKind::from_xml_tag(tag).map_err(|_| EditError::UnsupportedValue)?;
        Ok(Some(self.property_reference(&[(kind, name)])?))
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
        let editing = self.property_fields(id)?;
        let field = editing
            .properties
            .fields
            .iter()
            .find(|field| field.path == path)
            .ok_or(EditError::UnsupportedValue)?;
        if let ScalarSchema::Reference {
            nullable,
            ref domain,
            ..
        } = field.schema
        {
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
            if matches!(domain.as_str(), choice_fields::DOMAIN | type_links::DOMAIN) {
                return Ok(());
            }
            let parts = references::parts(value).ok_or(EditError::InvalidValue)?;
            self.property_reference(&parts)?;
        }
        Ok(())
    }
}
