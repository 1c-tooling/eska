//! Root descriptor detection, not a semantic validator for an entire export.

use crate::project::ProjectType;

const NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";

pub fn project_type(input: &str) -> Result<Option<ProjectType>, roxmltree::Error> {
    let document = roxmltree::Document::parse_with_options(
        input,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )?;
    let root = document.root_element();
    if !root.has_tag_name((NAMESPACE, "MetaDataObject")) {
        return Ok(None);
    }
    let mut objects = root.children().filter(roxmltree::Node::is_element);
    let Some(object) = objects.next() else {
        return Ok(None);
    };
    if objects.next().is_some() || object.tag_name().namespace() != Some(NAMESPACE) {
        return Ok(None);
    }
    let kind = match object.tag_name().name() {
        "Configuration" => {
            let extension = object
                .children()
                .filter(|node| node.has_tag_name((NAMESPACE, "Properties")))
                .flat_map(|node| node.children())
                .any(|node| {
                    node.has_tag_name((NAMESPACE, "ConfigurationExtensionPurpose"))
                        || node.has_tag_name((NAMESPACE, "ConfigurationExtensionCompatibilityMode"))
                });
            if extension {
                ProjectType::Extension
            } else {
                ProjectType::Configuration
            }
        }
        "ExternalDataProcessor" => ProjectType::Processing,
        "ExternalReport" => ProjectType::Report,
        _ => return Ok(None),
    };
    Ok(Some(kind))
}

#[cfg(test)]
mod tests {
    use super::{NAMESPACE, project_type};
    use crate::project::ProjectType;

    #[test]
    fn detects_all_types_with_namespaces_bom_and_comments() {
        for (tag, properties, expected) in [
            ("Configuration", "", ProjectType::Configuration),
            (
                "Configuration",
                "<m:ConfigurationExtensionPurpose>Patch</m:ConfigurationExtensionPurpose>",
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
