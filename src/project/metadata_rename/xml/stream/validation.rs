//! Streaming checks use the existing strict parser for distinct XML names and namespace declarations.

use std::collections::BTreeSet;

use quick_xml::{
    XmlVersion,
    events::BytesStart,
    name::{Namespace, NamespaceResolver, ResolveResult},
};

use super::{EditError, ReferenceRename};

/// A large spreadsheet repeats a small vocabulary; these bounded sets are local to one document.
#[derive(Default)]
pub(super) struct Validator {
    names: BTreeSet<String>,
    bindings: BTreeSet<String>,
}

impl Validator {
    /// Validate every attribute, including those irrelevant to this rename, before selecting candidates.
    pub fn start(
        &mut self,
        namespaces: &mut NamespaceResolver,
        element: &BytesStart<'_>,
        rename: &ReferenceRename,
    ) -> Result<bool, EditError> {
        self.qualified_name(element.name().into_inner())?;
        if element.name().into_inner().starts_with("xmlns:") {
            return Err(EditError::InvalidXml);
        }
        // NsReader binds raw attribute text; valid escaped reserved URIs need normalization first.
        namespaces
            .push(&BytesStart::new("r"))
            .map_err(|_| EditError::InvalidXml)?;
        let mut attributes = Vec::new();
        let mut candidate = false;
        for attribute in element.attributes() {
            let attribute = attribute.map_err(|_| EditError::InvalidXml)?;
            self.qualified_name(attribute.key.into_inner())?;
            if attribute.value.contains('<') {
                return Err(EditError::InvalidXml);
            }
            let value = attribute
                .normalized_value(XmlVersion::Explicit1_0)
                .map_err(|_| EditError::InvalidXml)?;
            characters(&value)?;
            candidate |= rename.mentions_name(&value);
            if let Some(prefix) = attribute.key.as_namespace_binding() {
                self.binding(attribute.key.into_inner(), &value)?;
                namespaces
                    .add(prefix, Namespace(&value))
                    .map_err(|_| EditError::InvalidXml)?;
            } else {
                attributes.push(attribute.key);
            }
        }
        namespace(&namespaces.resolve_element(element.name()).0)?;
        let mut expanded = BTreeSet::new();
        for key in attributes {
            let (resolved, local) = namespaces.resolve_attribute(key);
            if !expanded.insert((namespace(&resolved)?, local.into_inner())) {
                return Err(EditError::InvalidXml);
            }
        }
        Ok(candidate)
    }

    /// Names are checked by roxmltree rather than approximated with Rust's Unicode character predicates.
    fn qualified_name(&mut self, name: &str) -> Result<(), EditError> {
        let parts: Vec<_> = name.split(':').collect();
        if parts.len() > 2 {
            return Err(EditError::InvalidXml);
        }
        for part in parts {
            if self.names.contains(part) {
                continue;
            }
            let fragment = format!("<{part}/>");
            let parsed =
                roxmltree::Document::parse(&fragment).map_err(|_| EditError::InvalidXml)?;
            if parsed.root_element().tag_name().name() != part
                || parsed.root_element().attributes().len() != 0
            {
                return Err(EditError::InvalidXml);
            }
            if self.names.len() < 4096 && part.len() <= 512 {
                self.names.insert(part.to_owned());
            }
        }
        Ok(())
    }

    /// Strict parsing enforces the reserved namespace binding rules after decoding.
    fn binding(&mut self, key: &str, value: &str) -> Result<(), EditError> {
        let value = quick_xml::escape::escape(value);
        let fragment = format!("<r {key}=\"{value}\"/>");
        if self.bindings.contains(&fragment) {
            return Ok(());
        }
        roxmltree::Document::parse(&fragment).map_err(|_| EditError::InvalidXml)?;
        if self.bindings.len() < 256 && fragment.len() <= 2048 {
            self.bindings.insert(fragment);
        }
        Ok(())
    }

    /// XML declarations cannot be smuggled into a processing instruction with different casing.
    pub fn instruction(target: &str) -> Result<(), EditError> {
        if target.eq_ignore_ascii_case("xml") {
            return Err(EditError::InvalidXml);
        }
        roxmltree::Document::parse(&format!("<?{target}?><r/>"))
            .map_err(|_| EditError::InvalidXml)?;
        Ok(())
    }
}

/// Attribute namespaces must compare after XML normalization, including numeric character references.
const fn namespace<'a>(value: &ResolveResult<'a>) -> Result<&'a str, EditError> {
    match value {
        ResolveResult::Unbound => Ok(""),
        ResolveResult::Bound(value) => Ok(value.into_inner()),
        ResolveResult::Unknown(_) => Err(EditError::InvalidXml),
    }
}

/// UTF-8 already excludes surrogates; XML 1.0 additionally excludes controls and U+FFFE/U+FFFF.
pub(super) fn characters(input: &str) -> Result<(), EditError> {
    if input
        .bytes()
        .any(|byte| byte < 0x20 && !matches!(byte, b'\t' | b'\n' | b'\r'))
        || input.contains(['\u{fffe}', '\u{ffff}'])
    {
        return Err(EditError::InvalidXml);
    }
    Ok(())
}

/// Only the predefined XML entities and valid numeric character references are allowed without a DTD.
pub(super) fn entity(reference: &str) -> Result<(), EditError> {
    let source = format!("&{reference};");
    let value = quick_xml::escape::unescape(&source).map_err(|_| EditError::InvalidXml)?;
    characters(&value)
}
