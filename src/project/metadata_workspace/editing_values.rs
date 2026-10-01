//! Design-time values select only empty references, enum values or existing predefined data.

use super::{ObjectSummary, ProjectSession, PropertyEditError};
use crate::project::{
    configurator::TreeOptions,
    metadata_edit::{
        EditError, FieldStep, PropertyChange, ScalarSchema, ValueDomain, choice_parameters, types,
        value_schema,
    },
    metadata_model::{MetadataKind, NodeId, ObjectId, PropertyKey},
    metadata_parser::PropertiesMode,
};

/// A value token can denote an empty reference, so its metadata target is optional.
pub struct PropertyValueChoice {
    pub value: String,
    pub object: Option<ObjectSummary>,
}

impl ProjectSession {
    /// Enumerate valid values for one generated reference type declared on the edited attribute.
    ///
    /// # Errors
    /// Rejects unadvertised value types and missing or malformed referenced objects.
    pub fn property_value_choices(
        &mut self,
        id: &ObjectId,
        path: &[FieldStep],
        key: &PropertyKey,
    ) -> Result<Vec<PropertyValueChoice>, PropertyEditError> {
        let editing = self.property_fields(id)?;
        let field = editing
            .properties
            .fields
            .iter()
            .find(|field| field.path == path)
            .ok_or(EditError::UnsupportedValue)?;
        let ScalarSchema::Value {
            domain,
            types: allowed,
            ..
        } = &field.schema
        else {
            return Err(EditError::UnsupportedValue.into());
        };
        if !(allowed.iter().any(|choice| &choice.key == key)
            || (matches!(domain, Some(ValueDomain::ChoiceParameter))
                && choice_parameters::reference_type(key).is_some()))
        {
            return Err(EditError::InvalidValue.into());
        }
        let prefix = value_schema::reference_prefix(key).ok_or(EditError::InvalidValue)?;
        let (family, name) = key.name.split_once('.').ok_or(EditError::InvalidValue)?;
        let tag = types::REFERENCE_TYPES
            .iter()
            .find_map(|(candidate, tag)| (*candidate == family).then_some(*tag))
            .ok_or(EditError::InvalidValue)?;
        let kind = MetadataKind::from_xml_tag(tag).map_err(|_| EditError::InvalidValue)?;
        let owner = self.property_reference(&[(kind, name)])?;
        let mut result = vec![PropertyValueChoice {
            value: format!("{prefix}EmptyRef"),
            object: None,
        }];
        if kind == MetadataKind::Enum {
            self.children(&NodeId::Object(owner.id.clone()), TreeOptions::default())?;
            result.extend(
                self.objects
                    .values()
                    .filter(|object| {
                        object.kind == MetadataKind::EnumValue
                            && object.parent.as_ref() == Some(&owner.id)
                    })
                    .map(|object| PropertyValueChoice {
                        value: format!("{prefix}EnumValue.{}", object.name),
                        object: Some(object.clone()),
                    }),
            );
        } else if matches!(
            kind,
            MetadataKind::Catalog
                | MetadataKind::ChartOfAccounts
                | MetadataKind::ChartOfCalculationTypes
                | MetadataKind::ChartOfCharacteristicTypes
        ) {
            let parsed = self.load_predefined(&owner.id, PropertiesMode::Summary)?;
            let mut names = std::collections::BTreeMap::<&str, usize>::new();
            for object in &parsed.objects {
                *names.entry(object.metadata.name()).or_default() += 1;
            }
            result.extend(
                parsed
                    .objects
                    .iter()
                    .filter(|object| names[object.metadata.name()] == 1)
                    .map(|object| {
                        let metadata = &object.metadata;
                        PropertyValueChoice {
                            value: format!("{prefix}{}", metadata.name()),
                            object: Some(ObjectSummary {
                                id: metadata.id().clone(),
                                kind: metadata.kind(),
                                name: metadata.name().to_owned(),
                                uuid: Some(metadata.uuid().to_owned()),
                                parent: metadata.parent().cloned(),
                                synonyms: object.synonyms.clone(),
                            }),
                        }
                    }),
            );
        }
        result[1..].sort_by(|left, right| left.value.cmp(&right.value));
        Ok(result)
    }

    /// Primitive constraints are checked by the document; reference membership needs this project.
    pub(super) fn validate_property_value(
        &mut self,
        id: &ObjectId,
        path: &[FieldStep],
        change: &PropertyChange,
    ) -> Result<(), PropertyEditError> {
        if let PropertyChange::Value {
            key: Some(key),
            value,
        } = change
            && key.namespace.as_deref() == Some(types::CFG)
            && !self
                .property_value_choices(id, path, key)?
                .iter()
                .any(|choice| &choice.value == value)
        {
            return Err(EditError::InvalidValue.into());
        }
        Ok(())
    }
}
