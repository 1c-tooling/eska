//! One namespace envelope for validation and semantic reading of opaque native fragments.

use super::{Error, Namespace, XmlFragment, validation::scope};

/// Restore effective namespace bindings without materializing a native descriptor or project.
pub(super) fn wrap(value: &XmlFragment, inherited: &[Namespace]) -> Result<String, Error> {
    let context = scope(inherited, &value.namespaces)?;
    if value.xml.contains("<!DOCTYPE") || value.xml.contains("<!ENTITY") {
        return Err(Error::root("source_xml_dtd_forbidden"));
    }
    let mut wrapper = String::from("<preserved");
    for binding in &context {
        wrapper.push_str(" xmlns");
        if let Some(prefix) = &binding.prefix {
            wrapper.push(':');
            wrapper.push_str(prefix);
        }
        wrapper.push_str("=\"");
        wrapper.push_str(
            &binding
                .uri
                .replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;"),
        );
        wrapper.push('"');
    }
    wrapper.push('>');
    wrapper.push_str(&value.xml);
    wrapper.push_str("</preserved>");
    Ok(wrapper)
}
