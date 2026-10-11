//! Deterministic native metadata transport; opaque payload text is never reformatted.

use std::fmt::Write;

use super::{
    Child, Descriptor, Error, LineEnding, MetadataNode, Namespace, Property, PropertyValue,
    Section, XmlAttribute, XmlFragment, validation::scope,
};

const V8_NAMESPACE: &str = "http://v8.1c.ru/8.1/data/core";
const XSI_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Generate XML from semantic sections and preserved slots, using no filesystem state.
pub(super) fn encode(source: &Descriptor) -> Result<String, Error> {
    let mut writer = Writer {
        output: String::new(),
        eol: match source.xml.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        },
    };
    if source.xml.bom {
        writer.output.push('\u{feff}');
    }
    writer
        .output
        .push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    writer.output.push_str(writer.eol);
    writer.output.push_str("<MetaDataObject");
    writer.namespaces(&source.xml.namespaces);
    writer.attribute("version", &source.xml.version);
    writer.attributes(&source.xml.attributes);
    writer.output.push('>');
    writer.output.push_str(writer.eol);
    writer.object(&source.object, 1, &source.xml.namespaces)?;
    writer.output.push_str("</MetaDataObject>");
    writer.output.push_str(writer.eol);
    crate::project_core::native::check_envelope(&writer.output)
        .map_err(|_| Error::root("source_xml_export_invalid"))?;
    roxmltree::Document::parse_with_options(
        &writer.output,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|_| Error::root("source_xml_export_invalid"))?;
    Ok(writer.output)
}

struct Writer {
    output: String,
    eol: &'static str,
}

impl Writer {
    /// Preserve node-local scope while emitting the canonical native kind spelling.
    fn object(
        &mut self,
        node: &MetadataNode,
        indent: usize,
        inherited: &[Namespace],
    ) -> Result<(), Error> {
        let context = scope(inherited, &node.namespaces)?;
        self.indent(indent);
        self.output.push('<');
        self.output.push_str(node.kind.designer_tag());
        self.namespaces(&node.namespaces);
        self.attribute("uuid", node.uuid.as_str());
        self.attributes(&node.attributes);
        self.output.push('>');
        self.output.push_str(self.eol);
        for section in &node.sections {
            self.section(section, indent + 1, &context)?;
        }
        self.close(node.kind.designer_tag(), indent);
        Ok(())
    }

    /// Keep the native section order, including comments and otherwise unknown slots.
    fn section(
        &mut self,
        section: &Section,
        indent: usize,
        context: &[Namespace],
    ) -> Result<(), Error> {
        match section {
            Section::Properties(values) => {
                self.open("Properties", indent);
                for property in values {
                    self.property(property, indent + 1, context)?;
                }
                self.close("Properties", indent);
            }
            Section::Children(values) => {
                self.open("ChildObjects", indent);
                for value in values {
                    match value {
                        Child::Inline { object } => self.object(object, indent + 1, context)?,
                        Child::Reference {
                            metadata_kind,
                            name,
                        } => self.text_element(metadata_kind.designer_tag(), name, indent + 1),
                        Child::Preserved { fragment } => self.fragment(fragment, indent + 1)?,
                    }
                }
                self.close("ChildObjects", indent);
            }
            Section::Preserved(fragment) => self.fragment(fragment, indent)?,
        }
        Ok(())
    }

    /// Distinguish nil from empty and retain exact text and localized sequence ordering.
    fn property(
        &mut self,
        property: &Property,
        indent: usize,
        context: &[Namespace],
    ) -> Result<(), Error> {
        let (name, value) = match property {
            Property::Typed { name, value } => (name, value),
            Property::Preserved { fragment } => return self.fragment(fragment, indent),
        };
        let tag = name.as_str();
        match value {
            PropertyValue::Text(value) | PropertyValue::Enumeration(value) => {
                self.text_element(tag, value, indent);
            }
            PropertyValue::Boolean(value) => {
                self.text_element(tag, if *value { "true" } else { "false" }, indent);
            }
            PropertyValue::Integer(value) => self.text_element(tag, value.as_str(), indent),
            PropertyValue::Empty => {
                self.indent(indent);
                let _ = write!(self.output, "<{tag}/>{}", self.eol);
            }
            PropertyValue::Nil => {
                let (prefix, local) = prefix(context, XSI_NAMESPACE, "xsi");
                self.indent(indent);
                let _ = write!(self.output, "<{tag}");
                self.namespaces(&local);
                self.attribute(&format!("{prefix}:nil"), "true");
                self.output.push_str("/>");
                self.output.push_str(self.eol);
            }
            PropertyValue::Localized(values) => {
                let (prefix, local) = prefix(context, V8_NAMESPACE, "v8");
                self.indent(indent);
                let _ = write!(self.output, "<{tag}");
                self.namespaces(&local);
                self.output.push('>');
                self.output.push_str(self.eol);
                for value in values {
                    self.open(&format!("{prefix}:item"), indent + 1);
                    self.text_element(&format!("{prefix}:lang"), &value.language, indent + 2);
                    self.text_element(&format!("{prefix}:content"), &value.text, indent + 2);
                    self.close(&format!("{prefix}:item"), indent + 1);
                }
                self.close(tag, indent);
            }
        }
        Ok(())
    }

    /// Restore wrapper-inherited bindings on an opaque root without altering its interior bytes.
    fn fragment(&mut self, fragment: &XmlFragment, indent: usize) -> Result<(), Error> {
        self.indent(indent);
        self.output.push_str(&materialize_fragment(fragment)?);
        self.output.push_str(self.eol);
        Ok(())
    }

    /// Declare bindings in their stored order, including an explicit default namespace reset.
    fn namespaces(&mut self, bindings: &[Namespace]) {
        for binding in bindings {
            let name = binding
                .prefix
                .as_ref()
                .map_or_else(|| "xmlns".into(), |prefix| format!("xmlns:{prefix}"));
            self.attribute(&name, &binding.uri);
        }
    }

    /// Additional attribute spelling stays native; only XML escaping is regenerated.
    fn attributes(&mut self, values: &[XmlAttribute]) {
        for value in values {
            self.attribute(&value.name, &value.value);
        }
    }

    /// Escape whitespace as character references to avoid XML attribute normalization.
    fn attribute(&mut self, name: &str, value: &str) {
        self.output.push(' ');
        self.output.push_str(name);
        self.output.push_str("=\"");
        self.output.push_str(
            &escape(value)
                .replace('"', "&quot;")
                .replace('\n', "&#10;")
                .replace('\t', "&#9;"),
        );
        self.output.push('"');
    }

    /// Emit XML text without treating project version strings as numbers.
    fn text_element(&mut self, tag: &str, value: &str, indent: usize) {
        self.indent(indent);
        let _ = write!(self.output, "<{tag}>{}</{tag}>{}", escape(value), self.eol);
    }

    /// Generate only inter-element formatting; payload whitespace belongs to its value.
    fn open(&mut self, tag: &str, indent: usize) {
        self.indent(indent);
        let _ = write!(self.output, "<{tag}>{}", self.eol);
    }

    /// Close a native structural section using the selected document transport policy.
    fn close(&mut self, tag: &str, indent: usize) {
        self.indent(indent);
        let _ = write!(self.output, "</{tag}>{}", self.eol);
    }

    /// Native pretty printing uses tabs only outside preserved or typed text values.
    fn indent(&mut self, indent: usize) {
        self.output.extend(std::iter::repeat_n('\t', indent));
    }
}

/// Use a bound prefix when available; otherwise choose an unused local spelling.
fn prefix(context: &[Namespace], uri: &str, preferred: &str) -> (String, Vec<Namespace>) {
    if let Some(binding) = context
        .iter()
        .find(|binding| binding.uri == uri && binding.prefix.is_some())
    {
        return (binding.prefix.clone().unwrap_or_default(), Vec::new());
    }
    let mut candidate = preferred.to_owned();
    while context
        .iter()
        .any(|binding| binding.prefix.as_deref() == Some(&candidate))
    {
        candidate.push('_');
    }
    (
        candidate.clone(),
        vec![Namespace {
            prefix: Some(candidate),
            uri: uri.into(),
        }],
    )
}

/// Insert only bindings missing from the opaque element's own native start tag.
fn materialize_fragment(fragment: &XmlFragment) -> Result<String, Error> {
    if fragment.namespaces.is_empty() || fragment.xml.trim_start().starts_with("<!--") {
        return Ok(fragment.xml.clone());
    }
    let mut reader = quick_xml::Reader::from_str(&fragment.xml);
    let start = loop {
        match reader
            .read_event()
            .map_err(|_| Error::root("source_preserved_xml_invalid"))?
        {
            quick_xml::events::Event::Start(start) | quick_xml::events::Event::Empty(start) => {
                break start;
            }
            quick_xml::events::Event::Text(_) => {}
            _ => return Err(Error::root("source_preserved_xml_invalid")),
        }
    };
    let declared = start
        .attributes()
        .map(|attribute| attribute.map(|attribute| attribute.key.as_ref().to_owned()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::root("source_preserved_xml_invalid"))?;
    let bindings: Vec<_> = fragment
        .namespaces
        .iter()
        .filter(|binding| {
            let name = binding
                .prefix
                .as_ref()
                .map_or_else(|| "xmlns".into(), |prefix| format!("xmlns:{prefix}"));
            !declared.iter().any(|key| key == &name)
        })
        .cloned()
        .collect();
    if bindings.is_empty() {
        return Ok(fragment.xml.clone());
    }
    let offset = fragment
        .xml
        .find('<')
        .ok_or_else(|| Error::root("source_preserved_xml_invalid"))?
        + 1
        + start.name().as_ref().len();
    let mut writer = Writer {
        output: String::new(),
        eol: "\n",
    };
    writer.namespaces(&bindings);
    let mut result = fragment.xml.clone();
    result.insert_str(offset, &writer.output);
    Ok(result)
}

/// XML character references retain carriage returns that parsers would otherwise normalize.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\r', "&#13;")
}
