//! Typed native UI identity and layout precede authoring; opaque dialog bytes retain their current owner.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use super::Error;

mod kind;
mod native;
mod source;
#[cfg(test)]
mod tests;
mod validation;

pub use kind::FormElementKind;
pub use source::ManagedFormSource;

pub(super) const NAMESPACE: &str = "http://v8.1c.ru/8.3/xcf/logform";

/// Native decimal identity belongs to its explicit namespace; only the fixed root bar uses -1.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct FormId(String);

impl FormId {
    /// Reject noncanonical or out-of-profile IDs without normalizing their source spelling.
    ///
    /// # Errors
    /// Returns an invalid native ID; qualified names and UUIDs do not designate form elements.
    pub fn parse(value: &str) -> Result<Self, Error> {
        if value == "-1"
            || (!value.is_empty()
                && value.bytes().all(|byte| byte.is_ascii_digit())
                && (value == "0" || !value.starts_with('0')))
        {
            Ok(Self(value.to_owned()))
        } else {
            Err(Error::root("source_form_id_invalid"))
        }
    }

    /// Preserve the exact native decimal spelling for explicit namespace lookup.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for FormId {
    /// Persisted IDs obey the same lexical boundary as native XML and direct callers.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// A definition does not carry layout parentage or a second copy of its native ID.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormElement {
    pub kind: FormElementKind,
    pub name: String,
}

/// Root attributes and commands keep independent numeric namespaces and pinned native names.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FormNamedValue {
    pub name: String,
}

/// Fixed nodes remain attached to their owner; their own `ChildItems` are ordered normally.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FormFixedSlot {
    pub owner: Option<FormId>,
    pub element: FormId,
}

/// Only ordinary ordered edges belong to editable layout; definitions and fixed slots remain independent.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormLayout {
    pub roots: Vec<FormId>,
    pub children: BTreeMap<FormId, Vec<FormId>>,
}

/// This addressed read model does not replace XML payloads or claim a new authoring schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManagedFormLayout {
    pub elements: BTreeMap<FormId, FormElement>,
    pub attributes: BTreeMap<FormId, FormNamedValue>,
    pub commands: BTreeMap<FormId, FormNamedValue>,
    pub layout: FormLayout,
    pub fixed: Vec<FormFixedSlot>,
    pub addition_sources: BTreeMap<FormId, FormId>,
    pub local_commands: BTreeMap<FormId, FormId>,
}

impl ManagedFormLayout {
    /// Decode only the observed logform/2.20 UI shape and its proven local references.
    ///
    /// # Errors
    /// Returns malformed XML, unverified profile, duplicate identity or inconsistent layout/reference.
    pub fn from_xml(input: &str) -> Result<Self, Error> {
        native::decode(input)
    }

    /// Require one real parent, valid nesting, full reachability and unambiguous known reference targets.
    ///
    /// # Errors
    /// Returns duplicate/missing parents, dangling targets, invalid nesting or a layout cycle.
    pub fn validate(&self) -> Result<(), Error> {
        validation::validate(self)
    }

    /// A layout-only change preserves definitions, fixed owners and the accepted nearest-table context.
    ///
    /// # Errors
    /// Returns invalid layout, substituted definitions/bindings or a move across an unaccepted data context.
    pub fn validate_layout_change(&self, accepted: &Self) -> Result<(), Error> {
        self.validate()?;
        accepted.validate()?;
        if self.elements != accepted.elements
            || self.attributes != accepted.attributes
            || self.commands != accepted.commands
            || validation::fixed_bindings(self) != validation::fixed_bindings(accepted)
            || self.addition_sources != accepted.addition_sources
            || self.local_commands != accepted.local_commands
        {
            return Err(Error::root("source_form_identity_changed"));
        }
        if validation::table_contexts(self)? != validation::table_contexts(accepted)? {
            return Err(Error::root("source_form_data_context_changed"));
        }
        Ok(())
    }
}
