//! Designer XML paths and metadata documents projected into logical 1C object identifiers.

use std::collections::{BTreeMap, BTreeSet};

use gix::bstr::{BStr, ByteSlice};

use super::ProjectType;

const MD_NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";

/// One logical path part. `kind` is stable and localized only by the CLI layer.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MetadataPart {
    pub kind: &'static str,
    pub name: Option<String>,
}

/// A Configurator-style identifier grouped by its top-level metadata type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MetadataPath {
    pub group: &'static str,
    pub parts: Vec<MetadataPart>,
}

impl MetadataPath {
    /// Append a nested metadata child to an existing owner path.
    pub fn with_suffix(&self, suffix: &[MetadataPart]) -> Self {
        let mut parts = self.parts.clone();
        parts.extend_from_slice(suffix);
        Self {
            group: self.group,
            parts,
        }
    }
}

/// Resolve a project-relative Designer XML path when its ownership is unambiguous.
pub fn from_path(project_type: ProjectType, path: &BStr) -> Option<MetadataPath> {
    let path = path.to_str().ok()?;
    let components: Vec<_> = path.split('/').collect();
    if components.is_empty() {
        return None;
    }

    if components[0] == "Configuration.xml" {
        return Some(root_path("configuration", None));
    }
    if components[0] == "Ext" {
        return root_extension_path(project_type, &components);
    }
    if !components[0].contains('.')
        && components.len() >= 2
        && let Some(kind) = top_level_kind(components[0])
    {
        return top_level_path(kind, &components);
    }
    external_object_path(project_type, &components)
}

/// Tell whether a path is the main XML descriptor whose children can be compared semantically.
pub fn is_main_descriptor(project_type: ProjectType, path: &BStr) -> bool {
    let Ok(path) = path.to_str() else {
        return false;
    };
    let components: Vec<_> = path.split('/').collect();
    match components.as_slice() {
        ["Configuration.xml"] => true,
        [folder, file] => top_level_kind(folder).is_some() && has_extension(file, "xml"),
        [file] => {
            matches!(project_type, ProjectType::Processing | ProjectType::Report)
                && has_extension(file, "xml")
        }
        _ => false,
    }
}

/// Compare metadata child objects in two UTF-8 Designer XML descriptor snapshots.
pub fn changed_children(before: &[u8], after: &[u8]) -> Option<Vec<Vec<MetadataPart>>> {
    let before = parse_children(before)?;
    let after = parse_children(after)?;
    let mut keys: BTreeSet<_> = before.children.keys().cloned().collect();
    keys.extend(after.children.keys().cloned());
    let mut changed = Vec::new();
    if before.properties != after.properties {
        changed.push(Vec::new());
    }
    changed.extend(
        keys.into_iter()
            .filter(|key| before.children.get(key) != after.children.get(key)),
    );
    Some(changed)
}

/// Build the logical root and optional nested path for a top-level metadata object.
fn top_level_path(kind: &'static str, components: &[&str]) -> Option<MetadataPath> {
    let object = components
        .get(1)?
        .strip_suffix(".xml")
        .unwrap_or(components[1]);
    let base = root_path(kind, Some(object));
    nested_path(base, &components[2..])
}

/// Build the logical root for an external processing or report export.
fn external_object_path(project_type: ProjectType, components: &[&str]) -> Option<MetadataPath> {
    let kind = match project_type {
        ProjectType::Processing => "data-processor",
        ProjectType::Report => "report",
        ProjectType::Configuration | ProjectType::Extension => return None,
    };
    let object = components[0].strip_suffix(".xml").unwrap_or(components[0]);
    if object.is_empty() {
        return None;
    }
    nested_path(root_path(kind, Some(object)), &components[1..])
}

/// Resolve configuration-level modules and artifacts below `Ext`.
fn root_extension_path(project_type: ProjectType, components: &[&str]) -> Option<MetadataPath> {
    let kind = match project_type {
        ProjectType::Configuration | ProjectType::Extension => "configuration",
        ProjectType::Processing | ProjectType::Report => return None,
    };
    nested_path(root_path(kind, None), &components[1..])
}

/// Construct the first Configurator-style segment.
fn root_path(kind: &'static str, name: Option<&str>) -> MetadataPath {
    MetadataPath {
        group: kind,
        parts: vec![MetadataPart {
            kind,
            name: name.map(str::to_owned),
        }],
    }
}

/// Resolve known nested folders and module filenames, rejecting ambiguous leftovers.
fn nested_path(mut base: MetadataPath, components: &[&str]) -> Option<MetadataPath> {
    if components.is_empty() {
        return Some(base);
    }
    let components = if components.first() == Some(&"Ext") {
        &components[1..]
    } else {
        components
    };
    match components {
        [] => Some(base),
        [file] => {
            if has_extension(file, "xml") && base.parts.len() == 1 {
                return Some(base);
            }
            let module = module_kind(file)?;
            if !(base.group == "common-module" && module == "module") {
                base.parts.push(MetadataPart {
                    kind: module,
                    name: None,
                });
            }
            Some(base)
        }
        [collection, item] if has_extension(item, "xml") => {
            append_named_collection(&mut base, collection, item)?;
            Some(base)
        }
        [collection, item, "Ext", file] => {
            append_named_collection(&mut base, collection, item)?;
            if !matches!(*file, "Form.xml" | "Template.xml") {
                let module = module_kind(file)?;
                base.parts.push(MetadataPart {
                    kind: module,
                    name: None,
                });
            }
            Some(base)
        }
        ["Forms", item, "Ext", "Form", "Module.bsl"] => {
            append_named_collection(&mut base, "Forms", item)?;
            Some(base)
        }
        _ => None,
    }
}

/// Compare a filename extension without imposing platform case sensitivity.
fn has_extension(file: &str, expected: &str) -> bool {
    std::path::Path::new(file)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

/// Append a form, template or command name taken from its Designer XML folder.
fn append_named_collection(base: &mut MetadataPath, collection: &str, item: &str) -> Option<()> {
    let kind = match collection {
        "Forms" => "form",
        "Templates" => "template",
        "Commands" => "command",
        _ => return None,
    };
    base.parts.push(MetadataPart {
        kind,
        name: Some(item.strip_suffix(".xml").unwrap_or(item).to_owned()),
    });
    Some(())
}

/// Translate a standard Designer XML module filename into a logical child kind.
fn module_kind(file: &str) -> Option<&'static str> {
    Some(match file {
        "Module.bsl" => "module",
        "ObjectModule.bsl" => "object-module",
        "ManagerModule.bsl" => "manager-module",
        "RecordSetModule.bsl" => "record-set-module",
        "ValueManagerModule.bsl" => "value-manager-module",
        "ManagedApplicationModule.bsl" => "managed-application-module",
        "OrdinaryApplicationModule.bsl" => "ordinary-application-module",
        "SessionModule.bsl" => "session-module",
        "ExternalConnectionModule.bsl" => "external-connection-module",
        _ => return None,
    })
}

/// Map Designer XML top-level collection folders to stable metadata kind identifiers.
fn top_level_kind(folder: &str) -> Option<&'static str> {
    Some(match folder {
        "AccountingRegisters" => "accounting-register",
        "AccumulationRegisters" => "accumulation-register",
        "Bots" => "bot",
        "BusinessProcesses" => "business-process",
        "CalculationRegisters" => "calculation-register",
        "Catalogs" => "catalog",
        "ChartsOfAccounts" => "chart-of-accounts",
        "ChartsOfCalculationTypes" => "chart-of-calculation-types",
        "ChartsOfCharacteristicTypes" => "chart-of-characteristic-types",
        "CommandGroups" => "command-group",
        "CommonAttributes" => "common-attribute",
        "CommonCommands" => "common-command",
        "CommonForms" => "common-form",
        "CommonModules" => "common-module",
        "CommonPictures" => "common-picture",
        "CommonTemplates" => "common-template",
        "Constants" => "constant",
        "DataProcessors" => "data-processor",
        "DefinedTypes" => "defined-type",
        "DocumentJournals" => "document-journal",
        "Documents" => "document",
        "Enums" => "enum",
        "EventSubscriptions" => "event-subscription",
        "ExchangePlans" => "exchange-plan",
        "ExternalDataSources" => "external-data-source",
        "FilterCriteria" => "filter-criterion",
        "FunctionalOptions" => "functional-option",
        "FunctionalOptionsParameters" => "functional-option-parameter",
        "HTTPServices" => "http-service",
        "InformationRegisters" => "information-register",
        "IntegrationServices" => "integration-service",
        "Languages" => "language",
        "Reports" => "report",
        "Roles" => "role",
        "ScheduledJobs" => "scheduled-job",
        "Sequences" => "sequence",
        "SessionParameters" => "session-parameter",
        "SettingsStorages" => "settings-storage",
        "StyleItems" => "style-item",
        "Styles" => "style",
        "Subsystems" => "subsystem",
        "Tasks" => "task",
        "WebServices" => "web-service",
        "WSReferences" => "ws-reference",
        "XDTOPackages" => "xdto-package",
        _ => return None,
    })
}

struct ParsedMetadata {
    properties: String,
    children: BTreeMap<Vec<MetadataPart>, String>,
}

/// Parse only logical identities and normalized properties from one metadata descriptor.
fn parse_children(content: &[u8]) -> Option<ParsedMetadata> {
    let content = std::str::from_utf8(content)
        .ok()?
        .trim_start_matches('\u{feff}');
    let document = roxmltree::Document::parse(content).ok()?;
    let object = document
        .root_element()
        .children()
        .find(|node| node.is_element() && node.tag_name().namespace() == Some(MD_NAMESPACE))?;
    let properties = object
        .children()
        .find(|node| node.has_tag_name((MD_NAMESPACE, "Properties")))
        .map_or_else(String::new, fingerprint);
    let mut children = BTreeMap::new();
    collect_children(object, &mut Vec::new(), &mut children);
    Some(ParsedMetadata {
        properties,
        children,
    })
}

/// Recursively collect named children while preserving their owner hierarchy.
fn collect_children(
    owner: roxmltree::Node<'_, '_>,
    prefix: &mut Vec<MetadataPart>,
    children: &mut BTreeMap<Vec<MetadataPart>, String>,
) {
    let Some(container) = owner
        .children()
        .find(|node| node.has_tag_name((MD_NAMESPACE, "ChildObjects")))
    else {
        return;
    };
    for child in container.children().filter(roxmltree::Node::is_element) {
        let Some(kind) = child_kind(child.tag_name().name()) else {
            continue;
        };
        let Some(properties) = child
            .children()
            .find(|node| node.has_tag_name((MD_NAMESPACE, "Properties")))
        else {
            continue;
        };
        let Some(name) = properties
            .children()
            .find(|node| node.has_tag_name((MD_NAMESPACE, "Name")))
            .and_then(|node| node.text())
        else {
            continue;
        };
        prefix.push(MetadataPart {
            kind,
            name: Some(name.to_owned()),
        });
        children.insert(prefix.clone(), fingerprint(properties));
        collect_children(child, prefix, children);
        prefix.pop();
    }
}

/// Map metadata child XML tags to stable logical segment identifiers.
fn child_kind(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "Attribute" => "attribute",
        "TabularSection" => "tabular-section",
        "Dimension" => "dimension",
        "Resource" => "resource",
        "Requisite" => "requisite",
        "EnumValue" => "enum-value",
        "AccountingFlag" => "accounting-flag",
        "ExtDimensionAccountingFlag" => "ext-dimension-accounting-flag",
        "Recalculation" => "recalculation",
        "Column" => "column",
        _ => return None,
    })
}

/// Normalize an XML subtree structurally so formatting-only changes are ignored.
fn fingerprint(node: roxmltree::Node<'_, '_>) -> String {
    let mut value = String::new();
    fingerprint_into(node, &mut value);
    value
}

/// Append one normalized XML subtree to a deterministic string.
fn fingerprint_into(node: roxmltree::Node<'_, '_>, output: &mut String) {
    use std::fmt::Write as _;

    if node.is_element() {
        write!(
            output,
            "<{}:{}",
            node.tag_name().namespace().unwrap_or_default(),
            node.tag_name().name()
        )
        .expect("writing to String cannot fail");
        let mut attributes: Vec<_> = node.attributes().collect();
        attributes.sort_by_key(|attribute| (attribute.namespace(), attribute.name()));
        for attribute in attributes {
            write!(
                output,
                " {}:{}={:?}",
                attribute.namespace().unwrap_or_default(),
                attribute.name(),
                attribute.value()
            )
            .expect("writing to String cannot fail");
        }
        output.push('>');
        for child in node.children() {
            fingerprint_into(child, output);
        }
        output.push_str("</>");
    } else if node.is_text() {
        let text = node.text().unwrap_or_default();
        if !text.trim().is_empty() {
            write!(output, "{text:?}").expect("writing to String cannot fail");
        }
    }
}

#[cfg(test)]
mod tests {
    use gix::bstr::ByteSlice;

    use super::{changed_children, from_path};
    use crate::project::ProjectType;

    /// Designer paths resolve to Configurator ownership rather than filesystem layout.
    #[test]
    fn resolves_objects_modules_forms_and_fallback_boundaries() {
        let catalog = from_path(
            ProjectType::Configuration,
            b"Catalogs/Partners/Ext/ObjectModule.bsl".as_bstr(),
        )
        .unwrap();
        assert_eq!(catalog.group, "catalog");
        assert_eq!(catalog.parts[0].name.as_deref(), Some("Partners"));
        assert_eq!(catalog.parts[1].kind, "object-module");

        let form = from_path(
            ProjectType::Configuration,
            b"Catalogs/Partners/Forms/ItemForm/Ext/Form.xml".as_bstr(),
        )
        .unwrap();
        assert_eq!(form.parts[1].kind, "form");
        assert_eq!(form.parts[1].name.as_deref(), Some("ItemForm"));
        assert_eq!(
            from_path(
                ProjectType::Configuration,
                b"Catalogs/Partners/Forms/ItemForm/Ext/Form/Module.bsl".as_bstr(),
            ),
            Some(form.clone())
        );
        assert_eq!(
            from_path(
                ProjectType::Configuration,
                b"Catalogs/Partners/Forms/ItemForm.xml".as_bstr(),
            ),
            Some(form)
        );
        assert!(from_path(ProjectType::Configuration, b"notes/readme.txt".as_bstr()).is_none());
    }

    /// Child comparison identifies a changed attribute and ignores formatting around elements.
    #[test]
    fn detects_changed_child_properties() {
        let before = descriptor("Old");
        let after = descriptor("New");
        let changes = changed_children(before.as_bytes(), after.as_bytes()).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0][0].kind, "attribute");
        assert_eq!(changes[0][0].name.as_deref(), Some("Code"));
    }

    /// Build a minimal Designer XML owner with one attribute for semantic tests.
    fn descriptor(comment: &str) -> String {
        format!(
            r#"<MetaDataObject xmlns="{MD}"><Catalog><Properties><Name>Partners</Name></Properties><ChildObjects><Attribute><Properties><Name>Code</Name><Comment>{comment}</Comment></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>"#,
            MD = super::MD_NAMESPACE
        )
    }
}
