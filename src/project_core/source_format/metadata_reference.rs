//! Named metadata references keep one native spelling and resolve only against declared objects.

use serde::{Deserialize, Deserializer, Serialize};

use super::{Error, Namespace, PropertyName, validation::ncname};
use crate::project_core::MetadataKind;

mod native;
#[cfg(test)]
mod tests;

pub(super) use native::decode_list;

pub(super) const READABLE_NAMESPACE: &str = "http://v8.1c.ru/8.3/xcf/readable";
pub(super) const XSI_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Designer paths alternate exact metadata classes and names; UUID-only references stay opaque.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MetadataReference(String);

impl MetadataReference {
    /// Validate a bounded native path without case folding, filesystem guesses or UUID conversion.
    ///
    /// # Errors
    /// Returns `source_metadata_reference_invalid` for an unsupported class or name spelling.
    pub fn parse(value: &str) -> Result<Self, Error> {
        parts(value)?;
        Ok(Self(value.into()))
    }

    /// Preserve the exact native spelling as the only editable value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for MetadataReference {
    /// Every wire boundary enforces the same native path invariant.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// A homogeneous native reference list retains order and repeated entries without value copies.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataReferenceList {
    pub items: Vec<MetadataReference>,
    pub native: MetadataReferenceLayout,
}

/// Prefixes reconstruct Item and xsi:type syntax; they contain no metadata target values.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataReferenceLayout {
    pub item_prefix: String,
    pub xsi_prefix: String,
    pub type_prefix: String,
}

impl MetadataReferenceList {
    /// Empty lists use the existing Empty state; all explicit prefixes must resolve by URI.
    pub(super) fn validate(&self, context: &[Namespace]) -> Result<(), Error> {
        if self.items.is_empty() {
            return Err(Error::root("source_metadata_reference_list_invalid"));
        }
        for (prefix, uri) in [
            (&self.native.item_prefix, READABLE_NAMESPACE),
            (&self.native.xsi_prefix, XSI_NAMESPACE),
            (&self.native.type_prefix, READABLE_NAMESPACE),
        ] {
            if !ncname(prefix)
                || !context
                    .iter()
                    .any(|binding| binding.prefix.as_deref() == Some(prefix) && binding.uri == uri)
            {
                return Err(Error::root("source_metadata_reference_namespace_invalid"));
            }
        }
        Ok(())
    }
}

/// A property domain applies only to the reviewed owner and native cardinality.
#[derive(Clone, Copy)]
pub(super) enum ReferenceKind {
    One(Domain),
    Many(Domain),
}

/// Target classes and owner-local forms are structural constraints, independent of spelling.
#[derive(Clone, Copy)]
pub(super) enum Domain {
    Kind(MetadataKind),
    Form { common: bool },
    Owners,
    Registers,
    Content,
}

/// A coincidental property name in another metadata class does not establish a reference domain.
pub(super) const fn kind(name: PropertyName, owner: MetadataKind) -> Option<ReferenceKind> {
    use MetadataKind as K;
    use PropertyName as P;
    use ReferenceKind::{Many, One};

    Some(match (name, owner) {
        (P::DefaultLanguage, K::Configuration) => One(Domain::Kind(K::Language)),
        (P::DefaultStyle, K::Configuration) => One(Domain::Kind(K::Style)),
        (P::DefaultRoles, K::Configuration) => Many(Domain::Kind(K::Role)),
        (
            P::DefaultObjectForm,
            K::Catalog | K::ChartOfCharacteristicTypes | K::Document | K::ExchangePlan,
        )
        | (
            P::DefaultListForm,
            K::Catalog
            | K::ChartOfCharacteristicTypes
            | K::Document
            | K::ExchangePlan
            | K::Enum
            | K::InformationRegister
            | K::AccumulationRegister
            | K::AccountingRegister
            | K::CalculationRegister,
        )
        | (
            P::DefaultChoiceForm,
            K::Catalog | K::ChartOfCharacteristicTypes | K::Document | K::ExchangePlan | K::Enum,
        )
        | (
            P::DefaultFolderForm | P::DefaultFolderChoiceForm,
            K::Catalog | K::ChartOfCharacteristicTypes,
        )
        | (P::DefaultRecordForm, K::InformationRegister) => One(Domain::Form { common: false }),
        (P::DefaultForm, K::DataProcessor | K::Report | K::Constant | K::FilterCriterion) => {
            One(Domain::Form { common: true })
        }
        (P::Owners, K::Catalog) => Many(Domain::Owners),
        (P::RegisterRecords, K::Document | K::Sequence) => Many(Domain::Registers),
        (P::Content, K::Subsystem) => Many(Domain::Content),
        _ => return None,
    })
}

/// Owner-sensitive checks precede target lookup, so an existing object of another class is refused.
pub(super) fn validate_value(
    domain: Domain,
    owner: MetadataKind,
    value: &MetadataReference,
) -> Result<(), Error> {
    let parts = parts(value.as_str())?;
    let valid = match domain {
        Domain::Kind(kind) => parts.len() == 1 && parts[0].0 == kind,
        Domain::Owners => {
            parts.len() == 1
                && matches!(
                    parts[0].0,
                    MetadataKind::Catalog | MetadataKind::ChartOfCharacteristicTypes
                )
        }
        Domain::Registers => {
            parts.len() == 1
                && matches!(
                    parts[0].0,
                    MetadataKind::InformationRegister
                        | MetadataKind::AccumulationRegister
                        | MetadataKind::AccountingRegister
                        | MetadataKind::CalculationRegister
                )
        }
        Domain::Form { common } => {
            (common && parts.len() == 1 && parts[0].0 == MetadataKind::CommonForm)
                || (parts.len() == 2 && parts[0].0 == owner && parts[1].0 == MetadataKind::Form)
        }
        Domain::Content => true,
    };
    if !valid {
        return Err(Error::root("source_metadata_reference_domain_invalid"));
    }
    Ok(())
}

/// Bound path length and exact `NCNames` exclude whitespace, separators and undeclared XML aliases.
fn parts(value: &str) -> Result<Vec<(MetadataKind, &str)>, Error> {
    let tokens: Vec<_> = value.split('.').collect();
    if tokens.is_empty() || tokens.len() > 128 || !tokens.len().is_multiple_of(2) {
        return Err(invalid());
    }
    tokens
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let kind = MetadataKind::from_xml_tag(pair[0]).map_err(|_| invalid())?;
            if kind.designer_tag() != pair[0] || !ncname(pair[1]) {
                return Err(invalid());
            }
            Ok((kind, pair[1]))
        })
        .collect()
}

/// Strict value domains apply equally to XML promotion, authored YAML and permitted selection.
pub(super) fn validate_property(
    name: PropertyName,
    owner: MetadataKind,
    value: &super::PropertyValue,
    context: &[Namespace],
) -> Result<(), Error> {
    use super::PropertyValue;
    let Some(shape) = kind(name, owner) else {
        return Ok(());
    };
    match (shape, value) {
        (_, PropertyValue::Empty | PropertyValue::Nil) => Ok(()),
        (ReferenceKind::One(domain), PropertyValue::Reference(value)) => {
            validate_value(domain, owner, value)
        }
        (ReferenceKind::Many(domain), PropertyValue::ReferenceList(value)) => {
            value.validate(context)?;
            for item in &value.items {
                validate_value(domain, owner, item)?;
            }
            Ok(())
        }
        _ => Err(Error::root("source_property_type_invalid")),
    }
}

/// Invalid spelling never becomes an unresolved guessed metadata target.
fn invalid() -> Error {
    Error::root("source_metadata_reference_invalid")
}
