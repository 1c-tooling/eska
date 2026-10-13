//! Flat native stencils preserve opaque bytes without storing a second editable copy of definitions or layout.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{Error, FormElement, FormFixedSlot, FormId, FormLayout, ManagedFormLayout};
use crate::project_core::source_format::{Namespace, yaml};

mod decode;
mod render;
#[cfg(test)]
mod tests;

/// A versioned dialog codec; project file-map integration and supplier-policy authority remain separate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedFormSource {
    pub schema_version: u32,
    pub elements: BTreeMap<FormId, FormElement>,
    pub layout: FormLayout,
    native: NativeForm,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeForm {
    namespaces: Vec<Namespace>,
    root: Vec<Part>,
    elements: BTreeMap<FormId, Stencil>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stencil {
    /// Absence means the root scope, avoiding identical inherited bindings at every UI node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    namespaces: Option<Vec<Namespace>>,
    parts: Vec<Part>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Part {
    Xml { value: String },
    Name,
    Id,
    Class { prefix: String },
    Element { id: FormId },
    Children { frame: ChildFrame },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChildFrame {
    open: String,
    close: String,
    gaps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    empty: Option<String>,
}

impl ManagedFormSource {
    /// Extract real UI definitions and graph slots without embedding a full alternate Form.xml.
    ///
    /// # Errors
    /// Returns the native profile/graph failures or an unverified lexical binding.
    pub fn from_xml(input: &str) -> Result<Self, Error> {
        decode::decode(input)
    }

    /// Decode strict source YAML; opaque bindings cannot conceal an invalid UI graph.
    ///
    /// # Errors
    /// Returns strict YAML/schema errors, invalid bindings, scopes or native layout.
    pub fn from_yaml(input: &str) -> Result<Self, Error> {
        let value = serde_json::from_value(yaml::decode(input)?)
            .map_err(|_| Error::root("source_form_schema_invalid"))?;
        Self::validate(&value)?;
        Ok(value)
    }

    /// Serialize the sole editable definitions/layout separately from pinned native stencils.
    ///
    /// # Errors
    /// Returns invalid bindings, graph, scope or source serialization failures.
    pub fn to_yaml(&self) -> Result<String, Error> {
        self.validate()?;
        let value =
            serde_json::to_value(self).map_err(|_| Error::root("source_form_schema_invalid"))?;
        yaml::encode(&value)
    }

    /// Materialize one dialog for acceptance; this method does not grant supplier/support permissions.
    ///
    /// # Errors
    /// Returns malformed stencils, graph, known references or changed effective namespace scopes.
    pub fn to_xml(&self) -> Result<String, Error> {
        let xml = render::render(self)?;
        self.check_output(&xml)?;
        Ok(xml)
    }

    /// Derive data namespaces/references from their sole preserved native values and the current layout.
    ///
    /// # Errors
    /// Returns the same source/graph failures as XML materialization.
    pub fn layout_snapshot(&self) -> Result<ManagedFormLayout, Error> {
        let xml = render::render(self)?;
        self.check_output(&xml)
    }

    /// Validate the complete current model, without reading files or consulting an accepted base.
    ///
    /// # Errors
    /// Returns invalid source version, bindings, nesting, identities, references or namespace scopes.
    pub fn validate(&self) -> Result<(), Error> {
        self.layout_snapshot().map(|_| ())
    }

    /// Layout-only authoring keeps native stencils and independent definitions fixed to their accepted values.
    ///
    /// # Errors
    /// Returns substituted stencils/definitions, invalid layout or a changed data/namespace context.
    pub fn validate_layout_change(&self, accepted: &Self) -> Result<(), Error> {
        if self.native != accepted.native {
            return Err(binding());
        }
        self.layout_snapshot()?
            .validate_layout_change(&accepted.layout_snapshot()?)
    }

    /// Bound all render traversal to a validated single-parent graph and real declared collection slots.
    fn preflight(&self) -> Result<(), Error> {
        if self.schema_version != 1 {
            return Err(Error::root("source_form_schema_unsupported"));
        }
        if self.elements.keys().ne(self.native.elements.keys()) {
            return Err(binding());
        }
        let mut fixed = Vec::new();
        let mut collections = BTreeSet::new();
        for (owner, parts) in std::iter::once((None, &self.native.root)).chain(
            self.native
                .elements
                .iter()
                .map(|(id, stencil)| (Some(id), &stencil.parts)),
        ) {
            let mut names = 0;
            let mut ids = 0;
            let mut classes = 0;
            for part in parts {
                match part {
                    Part::Element { id } => fixed.push(FormFixedSlot {
                        owner: owner.cloned(),
                        element: id.clone(),
                    }),
                    Part::Children { frame } => {
                        if frame.gaps.is_empty() || !collections.insert(owner) {
                            return Err(binding());
                        }
                    }
                    Part::Name => names += 1,
                    Part::Id => ids += 1,
                    Part::Class { .. } => classes += 1,
                    Part::Xml { .. } => {}
                }
            }
            if owner.is_some() {
                if names != 1 || ids != 1 || !(1..=2).contains(&classes) {
                    return Err(binding());
                }
            } else if names != 0 || ids != 0 || classes != 0 {
                return Err(binding());
            }
        }
        if !self.layout.roots.is_empty() && !collections.contains(&None) {
            return Err(binding());
        }
        collections.remove(&None);
        let declared: BTreeSet<_> = self.layout.children.keys().map(Some).collect();
        if collections != declared {
            return Err(Error::root("source_form_layout_slot_invalid"));
        }
        ManagedFormLayout {
            elements: self.elements.clone(),
            attributes: BTreeMap::new(),
            commands: BTreeMap::new(),
            layout: self.layout.clone(),
            fixed,
            addition_sources: BTreeMap::new(),
            local_commands: BTreeMap::new(),
        }
        .validate()
    }

    /// Rendered definitions/layout must agree with the typed values, never hidden XML alternatives.
    fn check_output(&self, xml: &str) -> Result<ManagedFormLayout, Error> {
        let actual = ManagedFormLayout::from_xml(xml)?;
        if actual.elements != self.elements || actual.layout != self.layout {
            return Err(binding());
        }
        decode::check_scopes(self, xml)?;
        Ok(actual)
    }
}

/// Native fragments carry provenance, not an alternate editable identity or collection.
fn binding() -> Error {
    Error::root("source_form_binding_invalid")
}
