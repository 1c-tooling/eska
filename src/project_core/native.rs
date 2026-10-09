//! Root descriptor detection, not a semantic validator for an entire export.

use super::ProjectType;

const NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";

/// Detects the project type represented by a Designer XML root descriptor.
///
/// # Errors
/// Returns malformed XML or a forbidden DTD; this is not a project-wide validator.
pub fn project_type(input: &str) -> Result<Option<ProjectType>, roxmltree::Error> {
    let document = roxmltree::Document::parse_with_options(
        input,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )?;
    Ok(project_type_from_root(document.root_element()))
}

/// Inspect an already parsed root so metadata opening does not parse the same file again.
pub(crate) fn project_type_from_root(root: roxmltree::Node<'_, '_>) -> Option<ProjectType> {
    if !root.has_tag_name((NAMESPACE, "MetaDataObject")) {
        return None;
    }
    let mut objects = root.children().filter(roxmltree::Node::is_element);
    let object = objects.next()?;
    if objects.next().is_some() || object.tag_name().namespace() != Some(NAMESPACE) {
        return None;
    }
    let kind = match object.tag_name().name() {
        "Configuration" => {
            let extension = object
                .children()
                .filter(|node| node.has_tag_name((NAMESPACE, "Properties")))
                .flat_map(|node| node.children())
                .any(|node| node.has_tag_name((NAMESPACE, "ConfigurationExtensionPurpose")));
            if extension {
                ProjectType::Extension
            } else {
                ProjectType::Configuration
            }
        }
        "ExternalDataProcessor" => ProjectType::Processing,
        "ExternalReport" => ProjectType::Report,
        _ => return None,
    };
    Some(kind)
}

#[cfg(test)]
mod tests {
    use super::ProjectType;
    use super::{NAMESPACE, project_type};

    #[test]
    fn detects_all_types_with_namespaces_bom_comments_and_configuration_properties() {
        for (tag, properties, expected) in [
            (
                "Configuration",
                "<m:ConfigurationExtensionCompatibilityMode>Version8_3_27</m:ConfigurationExtensionCompatibilityMode>",
                ProjectType::Configuration,
            ),
            (
                "Configuration",
                "<m:ConfigurationExtensionPurpose>Patch</m:ConfigurationExtensionPurpose><m:ConfigurationExtensionCompatibilityMode>Version8_3_27</m:ConfigurationExtensionCompatibilityMode>",
                ProjectType::Extension,
            ),
            ("ExternalDataProcessor", "", ProjectType::Processing),
            ("ExternalReport", "", ProjectType::Report),
        ] {
            let xml = format!(
                "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?><!-- exported --><m:MetaDataObject xmlns:m=\"{NAMESPACE}\"><m:{tag}><m:Properties>{properties}</m:Properties></m:{tag}></m:MetaDataObject>"
            );
            assert_eq!(project_type(&xml).expect("XML"), Some(expected));
        }
    }

    #[test]
    fn ignores_nested_markers_foreign_namespaces_and_non_root_objects() {
        for xml in [
            "<MetaDataObject><Configuration/></MetaDataObject>".to_owned(),
            format!("<MetaDataObject xmlns=\"{NAMESPACE}\"><Report/></MetaDataObject>"),
            format!(
                "<MetaDataObject xmlns=\"{NAMESPACE}\"><Configuration/><ExternalReport/></MetaDataObject>"
            ),
            "<other><ExternalReport/></other>".to_owned(),
        ] {
            assert_eq!(project_type(&xml).expect("XML"), None);
        }
        let xml = format!(
            "<MetaDataObject xmlns=\"{NAMESPACE}\"><Configuration><ChildObjects><Properties><ConfigurationExtensionPurpose/></Properties></ChildObjects></Configuration></MetaDataObject>"
        );
        assert_eq!(
            project_type(&xml).expect("XML"),
            Some(ProjectType::Configuration)
        );
        let xml = format!(
            "<MetaDataObject xmlns=\"{NAMESPACE}\" xmlns:x=\"urn:foreign\"><Configuration><Properties><x:ConfigurationExtensionPurpose/></Properties></Configuration></MetaDataObject>"
        );
        assert_eq!(
            project_type(&xml).expect("XML"),
            Some(ProjectType::Configuration)
        );
    }

    #[test]
    fn rejects_malformed_xml_and_dtd() {
        for xml in [
            "<broken>",
            "<root/><root/>",
            "<!DOCTYPE root [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><root>&x;</root>",
        ] {
            assert!(project_type(xml).is_err());
        }
    }
}

/// A bounded envelope failure before recursive XML parsing.
#[derive(Debug)]
pub enum EnvelopeError {
    TooLarge,
    TooDeep,
    Xml(quick_xml::Error),
}

pub(crate) const MAX_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_DEPTH: usize = 64;

/// Bound nesting before entering roxmltree's recursive tokenizer, including debug builds.
///
/// # Errors
/// Returns an excessive byte/depth limit or a streaming XML envelope error.
pub fn check_envelope(input: &str) -> Result<(), EnvelopeError> {
    use quick_xml::events::Event;
    if input.len() > MAX_BYTES {
        return Err(EnvelopeError::TooLarge);
    }
    let mut reader = quick_xml::Reader::from_str(input);
    let mut depth = 0_usize;
    loop {
        match reader.read_event().map_err(EnvelopeError::Xml)? {
            Event::Start(_) => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(EnvelopeError::TooDeep);
                }
            }
            Event::Empty(_) if depth == MAX_DEPTH => return Err(EnvelopeError::TooDeep),
            Event::End(_) => {
                depth = depth.saturating_sub(1);
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

/// Recognize standard Designer BSL implementation module names.
#[must_use]
pub fn is_module_path(path: &std::path::Path) -> bool {
    if !matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("bsl" | "bin")
    ) {
        return false;
    }
    matches!(
        path.file_stem().and_then(|value| value.to_str()),
        Some(
            "Module"
                | "ObjectModule"
                | "ManagerModule"
                | "RecordSetModule"
                | "ValueManagerModule"
                | "ManagedApplicationModule"
                | "OrdinaryApplicationModule"
                | "SessionModule"
                | "ExternalConnectionModule"
                | "CommandModule"
        )
    )
}
