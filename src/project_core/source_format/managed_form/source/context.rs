//! Preserved native fragments point to current YAML nodes, without claiming decoded scalar subranges.

use std::ops::Range;

use yaml_edit::{AsYaml, YamlFile, YamlNode};

use super::{Error, FormId, ManagedFormSource, binding, render};
use crate::project_core::source_format::yaml;

pub(super) struct Fragment {
    pub(super) native: Range<usize>,
    pub(super) owner: Option<FormId>,
    pub(super) index: usize,
}

pub(super) struct ReferenceView {
    pub(super) xml: String,
    pub(super) fragments: Vec<Fragment>,
}

/// One preserved XML chunk belongs to one exact CST scalar; quoting and folding are not byte identities.
pub(in crate::project_core::source_format) struct FormContext {
    pub(in crate::project_core::source_format) xml: String,
    pub(in crate::project_core::source_format) locations: Vec<(Range<usize>, Range<usize>)>,
}

impl ManagedFormSource {
    /// Render only reference markup from this same current authoring input, never neighboring query values.
    pub(in crate::project_core::source_format) fn query_reference_context(
        &self,
        text: &str,
        max_bytes: u64,
    ) -> Result<FormContext, Error> {
        let (value, tree) = yaml::decode_with_tree(text)?;
        if value.get("form") != Some(&serde_json::to_value(self).map_err(|_| binding())?) {
            return Err(binding());
        }
        let view = render::render_reference(self, max_bytes, true)?;
        self.check_output(&view.xml)?;
        let locations = view
            .fragments
            .iter()
            .map(|fragment| Ok((fragment.native.clone(), scalar_range(&tree, fragment)?)))
            .collect::<Result<_, Error>>()?;
        Ok(FormContext {
            xml: view.xml,
            locations,
        })
    }
}

/// Renderer addresses come from real part indices and owner IDs, never XML substring searches.
fn scalar_range(tree: &YamlFile, fragment: &Fragment) -> Result<Range<usize>, Error> {
    let document = tree.document().ok_or_else(binding)?;
    let root = document.as_mapping().ok_or_else(binding)?;
    let form_node = root.get("form").ok_or_else(binding)?;
    let form = form_node.as_mapping().ok_or_else(binding)?;
    let native_node = form.get("native").ok_or_else(binding)?;
    let native = native_node.as_mapping().ok_or_else(binding)?;
    let sequence_node = if let Some(owner) = &fragment.owner {
        let collection = native.get("elements").ok_or_else(binding)?;
        let definitions = collection.as_mapping().ok_or_else(binding)?;
        let selected = definitions.get(owner.as_str()).ok_or_else(binding)?;
        let stencil = selected.as_mapping().ok_or_else(binding)?;
        stencil.get("parts")
    } else {
        native.get("root")
    }
    .ok_or_else(binding)?;
    let parts = sequence_node.as_sequence().ok_or_else(binding)?;
    let part_node = parts.get(fragment.index).ok_or_else(binding)?;
    let part = part_node.as_mapping().ok_or_else(binding)?;
    let node = part.get("value").ok_or_else(binding)?;
    if !matches!(node, YamlNode::Scalar(_)) {
        return Err(binding());
    }
    let range = node.as_node().ok_or_else(binding)?.text_range();
    Ok(usize::from(range.start())..usize::from(range.end()))
}
