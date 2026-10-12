//! Typed descriptions project directly into the shared metadata value model, without XML parsing.

use super::{CORE_NAMESPACE, TypeDescription, TypePart, qualified_name};
use crate::project_core::{
    metadata_model::{MetadataProperty, MetadataValue, PropertyKey},
    source_format::{Error, Namespace},
};

impl TypeDescription {
    /// Reproduce native record order and namespace-resolved `QName` values for every consumer.
    pub(crate) fn to_metadata(&self, context: &[Namespace]) -> Result<MetadataValue, Error> {
        let mut values = Vec::new();
        for part in &self.native.parts {
            values.push(match part {
                TypePart::Alternative(index) => {
                    let value = self
                        .data
                        .alternatives
                        .get(*index)
                        .ok_or_else(|| Error::root("source_type_layout_invalid"))?;
                    let (namespace, name) = qualified_name(&value.name, context)?;
                    property(
                        value.kind.tag(),
                        MetadataValue::QualifiedText {
                            text: value.name.clone(),
                            key: PropertyKey {
                                namespace: Some(namespace.into()),
                                name: name.into(),
                            },
                        },
                    )
                }
                TypePart::String => group(
                    "StringQualifiers",
                    self.native
                        .string_fields
                        .iter()
                        .map(|field| self.string_field(*field)),
                )?,
                TypePart::Number => group(
                    "NumberQualifiers",
                    self.native
                        .number_fields
                        .iter()
                        .map(|field| self.number_field(*field)),
                )?,
                TypePart::Date => group(
                    "DateQualifiers",
                    self.native
                        .date_fields
                        .iter()
                        .map(|field| self.date_field(*field)),
                )?,
            });
        }
        Ok(MetadataValue::Record(values))
    }
}

/// A missing binding is an error rather than an omitted native semantic field.
fn group<'a>(
    name: &str,
    fields: impl Iterator<Item = Option<(&'static str, &'a str)>>,
) -> Result<MetadataProperty, Error> {
    let values = fields
        .map(|field| {
            let (name, value) = field.ok_or_else(|| Error::root("source_type_layout_invalid"))?;
            Ok(property(name, MetadataValue::Text(value.into())))
        })
        .collect::<Result<_, Error>>()?;
    Ok(property(name, MetadataValue::Record(values)))
}

/// Native core fields carry no inferred XML attributes or default values.
fn property(name: &str, value: MetadataValue) -> MetadataProperty {
    MetadataProperty {
        key: PropertyKey {
            namespace: Some(CORE_NAMESPACE.into()),
            name: name.into(),
        },
        qualifiers: Vec::new(),
        value,
    }
}
