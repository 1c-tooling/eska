//! Source references keep query values outside XML and require a complete, explicit resolver.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    ops::Range,
};

use roxmltree::Node;

use super::{Error, NativeQueryDocument, QueryProfile, QuerySlot, native, stencil};
use crate::project_core::source_format::{SourcePath, validation};

/// The source-only protocol version belongs to its namespace, independently of project schema versions.
pub const QUERY_REFERENCE_NAMESPACE: &str = "urn:eska:query:1";

/// Lowered XML and separate plain query values have a single source for every declared slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractedQuerySource {
    xml: String,
    queries: BTreeMap<SourcePath, String>,
}

impl ExtractedQuerySource {
    /// Source XML contains references and inverse recipes, never the extracted text.
    #[must_use]
    pub fn xml(&self) -> &str {
        &self.xml
    }

    /// Values retain decoded text without XML escaping or language formatting.
    #[must_use]
    pub const fn queries(&self) -> &BTreeMap<SourcePath, String> {
        &self.queries
    }

    /// Publication transfers ownership of the XML and values without keeping alternate editable copies.
    #[must_use]
    pub fn into_parts(self) -> (String, BTreeMap<SourcePath, String>) {
        (self.xml, self.queries)
    }
}

/// Scoped native identity and an explicit source link remain separate from project trust and UUID context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryReference {
    pub slot: QuerySlot,
    pub source: SourcePath,
    frame: stencil::Frame,
}

impl QueryReference {
    /// Source XML coordinates belong to the exact parsed text; no filesystem or permission lookup is implied.
    #[must_use]
    pub fn source_range(&self) -> Range<usize> {
        self.frame.range.clone()
    }
}

/// Values are absent from this source representation; all declared links must resolve before native output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryReferenceDocument {
    profile: QueryProfile,
    parts: Vec<String>,
    references: Vec<QueryReference>,
}

impl NativeQueryDocument {
    /// Replace every declared text with one versioned link and transfer its sole value to a plain source.
    ///
    /// The caller selects paths; project ownership, safe filesystem reads and baseline policy are separate.
    ///
    /// # Errors
    /// Returns incomplete bindings, non-SDBL paths, case collisions or an invalid generated reference.
    pub fn extract(
        self,
        paths: &BTreeMap<QuerySlot, SourcePath>,
    ) -> Result<ExtractedQuerySource, Error> {
        if paths.len() != self.queries.len()
            || self
                .queries
                .iter()
                .any(|query| !paths.contains_key(&query.slot))
        {
            return Err(invalid());
        }
        validate_paths(paths.values())?;
        let mut parts = self.parts.into_iter();
        let mut xml = String::new();
        let mut queries = BTreeMap::new();
        for query in self.queries {
            xml.push_str(&parts.next().ok_or_else(invalid)?);
            let path = paths.get(&query.slot).ok_or_else(invalid)?;
            xml.push_str(&query.frame.reference(path));
            queries.insert(path.clone(), query.text);
        }
        xml.push_str(&parts.next().ok_or_else(invalid)?);
        let checked = QueryReferenceDocument::from_xml(&xml)?;
        if checked.profile != self.profile
            || checked.references.len() != paths.len()
            || checked
                .references
                .iter()
                .any(|reference| paths.get(&reference.slot) != Some(&reference.source))
        {
            return Err(invalid());
        }
        Ok(ExtractedQuerySource { xml, queries })
    }
}

impl QueryReferenceDocument {
    /// Read only the exact versioned, self-contained source protocol in known native query slots.
    ///
    /// Inline text, unmatched markup and ambiguous links cannot conceal a second authoring value.
    ///
    /// # Errors
    /// Returns unsupported protocol/profile, invalid recipes or duplicate/missing bindings.
    pub fn from_xml(input: &str) -> Result<Self, Error> {
        let document = native::parse(input)?;
        let mut references = Vec::new();
        let mut nodes = HashSet::new();
        let mut parts = Vec::new();
        let mut cursor = 0;
        for slot in native::slots(document.root_element())? {
            let marker = marker(slot.node)?;
            nodes.insert(marker.id());
            let source = SourcePath::parse(marker.attribute("source").ok_or_else(invalid)?)?;
            let frame = stencil::Frame::from_reference(slot.node, marker, input)?;
            parts.push(
                input
                    .get(cursor..frame.range.start)
                    .ok_or_else(invalid)?
                    .into(),
            );
            cursor = frame.range.end;
            references.push(QueryReference {
                slot: slot.slot,
                source,
                frame,
            });
        }
        parts.push(input.get(cursor..).ok_or_else(invalid)?.into());
        for node in document.descendants().filter(Node::is_element) {
            if node
                .namespaces()
                .any(|namespace| namespace.uri() == QUERY_REFERENCE_NAMESPACE)
                && !nodes.contains(&node.id())
            {
                return Err(invalid());
            }
        }
        validate_paths(references.iter().map(|reference| &reference.source))?;
        Ok(Self {
            profile: native::profile(document.root_element())?,
            parts,
            references,
        })
    }

    /// Context comes from the actual dataset/attribute scopes around each link.
    #[must_use]
    pub fn references(&self) -> &[QueryReference] {
        &self.references
    }

    /// The actual root disambiguates empty templates from empty form dialogs.
    #[must_use]
    pub const fn profile(&self) -> QueryProfile {
        self.profile
    }

    /// Materialize all links from explicit values; the source markup never reaches platform XML.
    ///
    /// # Errors
    /// Returns missing/extra sources, XML characters or a failed native binding/value check.
    pub fn materialize(&self, values: &BTreeMap<SourcePath, String>) -> Result<String, Error> {
        if values.len() != self.references.len()
            || self
                .references
                .iter()
                .any(|reference| !values.contains_key(&reference.source))
        {
            return Err(invalid());
        }
        self.render(|reference| values.get(&reference.source).map(String::as_str))
    }

    /// Project consumers borrow the already validated complete source set without cloning query values.
    pub(in crate::project_core::source_format) fn render<'a>(
        &'a self,
        value: impl Fn(&'a QueryReference) -> Option<&'a str>,
    ) -> Result<String, Error> {
        let mut output = String::new();
        for (part, reference) in self.parts.iter().zip(&self.references) {
            output.push_str(part);
            let text = value(reference).ok_or_else(invalid)?;
            validation::xml_text(text)?;
            output.push_str(&reference.frame.render(text));
        }
        output.push_str(self.parts.last().ok_or_else(invalid)?);
        let checked = NativeQueryDocument::from_xml(&output)?;
        if checked.profile() != self.profile
            || checked.queries().len() != self.references.len()
            || checked
                .queries()
                .iter()
                .zip(&self.references)
                .any(|(actual, reference)| {
                    actual.slot != reference.slot || value(reference) != Some(actual.text.as_str())
                })
        {
            return Err(invalid());
        }
        Ok(output)
    }
}

/// A marker has no value, children or unrelated attributes; only source whitespace can surround it.
fn marker<'a, 'input>(node: Node<'a, 'input>) -> Result<Node<'a, 'input>, Error> {
    let mut markers = node.children().filter(|child| {
        !child.is_text() || child.text().is_some_and(|text| !text.trim().is_empty())
    });
    let marker = markers.next().ok_or_else(invalid)?;
    if markers.next().is_some()
        || !marker.has_tag_name((QUERY_REFERENCE_NAMESPACE, "query"))
        || marker.children().next().is_some()
        || marker.attributes().len() != 7
        || marker.attributes().any(|attribute| {
            attribute.namespace().is_some()
                || !matches!(
                    attribute.name(),
                    "source" | "coding" | "eol" | "cr" | "quotes" | "apostrophes" | "empty"
                )
        })
    {
        return Err(invalid());
    }
    Ok(marker)
}

/// One physical plain source belongs to one slot; case aliases cannot overwrite another query.
fn validate_paths<'a>(paths: impl Iterator<Item = &'a SourcePath>) -> Result<(), Error> {
    let mut unique = BTreeSet::new();
    for path in paths {
        if !path
            .as_path()
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sdbl"))
            || !unique.insert(path.as_str().to_lowercase())
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Source markup does not inherit validity or authority from a familiar local name.
pub(super) fn invalid() -> Error {
    Error::root("source_query_reference_invalid")
}
