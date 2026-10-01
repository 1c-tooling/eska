//! Choice source selection and validation share a fresh descriptor-scoped inventory.

use super::{ObjectSummary, ProjectSession, PropertyEditError, PropertyReferenceChoice};
use crate::project::{
    metadata_edit::{EditError, FieldStep},
    metadata_model::MetadataKind,
    metadata_parser::{self, PropertiesMode},
};

impl ProjectSession {
    /// Same-file fields are resolved from current XML rather than potentially stale tree entries.
    pub(super) fn choice_parameter_fields(
        &mut self,
        id: &crate::project::metadata_model::ObjectId,
        path: &[FieldStep],
    ) -> Result<Vec<PropertyReferenceChoice>, PropertyEditError> {
        let (location, input) = self.editing_source(id)?;
        let document = self.editing_document(id, &location, &input)?;
        if self.object(id)?.kind == MetadataKind::Constant {
            return self.constant_choice_fields(id, path);
        }
        let root_id = self.source.root().id();
        let (_, root) = self.editing_source(root_id)?;
        let root = roxmltree::Document::parse(&root).map_err(|_| EditError::InvalidXml)?;
        let compatibility = root
            .descendants()
            .find(|node| {
                node.has_tag_name(("http://v8.1c.ru/8.3/MDClasses", "CompatibilityMode"))
                    && node
                        .parent()
                        .and_then(|node| node.parent())
                        .is_some_and(|node| {
                            node.has_tag_name(("http://v8.1c.ru/8.3/MDClasses", "Configuration"))
                        })
            })
            .and_then(|node| node.text())
            .unwrap_or_default();
        Ok(document
            .choice_fields(path, compatibility)?
            .into_iter()
            .map(|field| {
                let metadata = field.metadata;
                PropertyReferenceChoice {
                    value: field.value,
                    standard_attribute: field.standard_attribute,
                    object: ObjectSummary {
                        id: metadata.id().clone(),
                        kind: metadata.kind(),
                        name: metadata.name().to_owned(),
                        parent: metadata.parent().cloned(),
                        uuid: Some(metadata.uuid().to_owned()),
                        synonyms: field.synonyms,
                    },
                }
            })
            .collect())
    }

    /// Constants use other declared constants; each candidate is read again before being offered.
    fn constant_choice_fields(
        &mut self,
        id: &crate::project::metadata_model::ObjectId,
        path: &[FieldStep],
    ) -> Result<Vec<PropertyReferenceChoice>, PropertyEditError> {
        let editing = self.property_fields(id)?;
        let mut choices = Vec::new();
        let (_, root) = self.editing_source(self.source.root().id())?;
        let root = metadata_parser::parse(&root, None, PropertiesMode::Summary)
            .map_err(|_| EditError::InvalidXml)?;
        for reference in root
            .references
            .iter()
            .filter(|reference| reference.kind == MetadataKind::Constant && &reference.id != id)
        {
            let value = format!("Constant.{}", reference.name);
            let duplicate = editing.properties.fields.iter().any(|field| {
                field.path != path
                    && field.path.len() == path.len()
                    && field.path[..path.len() - 2] == path[..path.len() - 2]
                    && field.path.last() == path.last()
                    && crate::project::metadata_rename::same_name(&field.value, &value)
            });
            if duplicate {
                continue;
            }
            let (_, input) = self.editing_source(&reference.id)?;
            let parsed = metadata_parser::parse(&input, None, PropertiesMode::Summary)
                .map_err(|_| EditError::InvalidXml)?;
            let object = parsed
                .objects
                .into_iter()
                .find(|object| object.metadata.id() == &reference.id)
                .ok_or(EditError::UnsupportedValue)?;
            let metadata = object.metadata;
            choices.push(PropertyReferenceChoice {
                value,
                object: ObjectSummary {
                    id: metadata.id().clone(),
                    kind: metadata.kind(),
                    name: metadata.name().to_owned(),
                    parent: metadata.parent().cloned(),
                    uuid: Some(metadata.uuid().to_owned()),
                    synonyms: object.synonyms,
                },
                standard_attribute: None,
            });
        }
        choices.sort_by(|left, right| left.value.cmp(&right.value));
        Ok(choices)
    }
}
