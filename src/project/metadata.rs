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

/// Tell whether a path is a metadata descriptor represented as a logical object.
pub fn is_object_descriptor(project_type: ProjectType, path: &BStr) -> bool {
    let Ok(path) = path.to_str() else {
        return false;
    };
    let components: Vec<_> = path.split('/').collect();
    match project_type {
        ProjectType::Configuration | ProjectType::Extension => match components.as_slice() {
            ["Configuration.xml"] => true,
            [collection, file]
                if top_level_kind(collection).is_some() && has_extension(file, "xml") =>
            {
                true
            }
            [collection, _owner, nested @ ..] if top_level_kind(collection).is_some() => {
                is_nested_descriptor(nested)
            }
            _ => false,
        },
        ProjectType::Processing | ProjectType::Report => match components.as_slice() {
            [file] if has_extension(file, "xml") => true,
            ["Forms" | "Templates" | "Commands", file] if has_extension(file, "xml") => true,
            [_owner, nested @ ..] => is_nested_descriptor(nested),
            _ => false,
        },
    }
}

/// Recognize a descriptor below a named nested metadata collection.
fn is_nested_descriptor(components: &[&str]) -> bool {
    match components {
        ["Forms" | "Templates" | "Commands" | "Subsystems", file] => has_extension(file, "xml"),
        ["Subsystems", _owner, nested @ ..] => is_nested_descriptor(nested),
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

/// Resolve nested metadata collections and Designer-owned artifact files.
fn nested_path(mut base: MetadataPath, components: &[&str]) -> Option<MetadataPath> {
    if components.is_empty() {
        return Some(base);
    }
    let components = if components.first() == Some(&"Ext") {
        &components[1..]
    } else {
        components
    };
    if components.is_empty() {
        return Some(base);
    }
    if let [collection, item, rest @ ..] = components
        && let Some(kind) = nested_collection_kind(base.group, collection)
    {
        base.parts.push(MetadataPart {
            kind,
            name: Some(item.strip_suffix(".xml").unwrap_or(item).to_owned()),
        });
        if has_extension(item, "xml") {
            return rest.is_empty().then_some(base);
        }
        return nested_path(base, rest);
    }
    if let [file] = components
        && let Some(module) = module_kind(file)
    {
        if !module_belongs_to_owner(&base, module) {
            base.parts.push(MetadataPart {
                kind: module,
                name: None,
            });
        }
        return Some(base);
    }
    is_owner_artifact(&base, components).then_some(base)
}

/// Resolve a nested collection to the Configurator node it introduces.
fn nested_collection_kind(owner: &str, collection: &str) -> Option<&'static str> {
    Some(match collection {
        "Forms" => "form",
        "Templates" => "template",
        "Commands" => "command",
        "Subsystems" if owner == "subsystem" => "subsystem",
        _ => return None,
    })
}

/// Compare a filename extension without imposing platform case sensitivity.
fn has_extension(file: &str, expected: &str) -> bool {
    std::path::Path::new(file)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

/// Translate a standard Designer XML module filename into a logical child kind.
fn module_kind(file: &str) -> Option<&'static str> {
    let stem = file
        .strip_suffix(".bsl")
        .or_else(|| file.strip_suffix(".bin"))?;
    Some(match stem {
        "Module" => "module",
        "ObjectModule" => "object-module",
        "ManagerModule" => "manager-module",
        "RecordSetModule" => "record-set-module",
        "ValueManagerModule" => "value-manager-module",
        "ManagedApplicationModule" => "managed-application-module",
        "OrdinaryApplicationModule" => "ordinary-application-module",
        "SessionModule" => "session-module",
        "ExternalConnectionModule" => "external-connection-module",
        _ => return None,
    })
}

/// Tell whether a module file is implementation of the owner rather than a child node.
fn module_belongs_to_owner(path: &MetadataPath, module: &str) -> bool {
    (path.group == "common-module" || is_form_owner(path)) && module == "module"
}

/// Recognize Designer files whose logical identity is their existing metadata owner.
fn is_owner_artifact(path: &MetadataPath, components: &[&str]) -> bool {
    match components {
        [file] if has_extension(file, "xml") => true,
        ["Help", _rest @ ..] => true,
        ["Form", _rest @ ..] if is_form_owner(path) => true,
        ["Form.bin"] if is_form_owner(path) => true,
        ["Template", _rest @ ..] if is_template_owner(path) => true,
        [file] if is_template_owner(path) && file.starts_with("Template.") => true,
        ["Picture", _rest @ ..] if path.group == "common-picture" => true,
        ["Package.bin"] if path.group == "xdto-package" => true,
        [file] if path.group == "ws-reference" && has_extension(file, "xsd") => true,
        ["CommandModule.bsl"] if is_command_owner(path) => true,
        components if path.group == "configuration" => is_configuration_artifact(components),
        _ => false,
    }
}

/// Forms own their module and embedded item resources in the Configurator tree.
fn is_form_owner(path: &MetadataPath) -> bool {
    path.group == "common-form" || path.parts.last().is_some_and(|part| part.kind == "form")
}

/// Template payload files belong to the named template node.
fn is_template_owner(path: &MetadataPath) -> bool {
    path.group == "common-template"
        || path
            .parts
            .last()
            .is_some_and(|part| part.kind == "template")
}

/// Command modules are implementation of the named command node.
fn is_command_owner(path: &MetadataPath) -> bool {
    path.group == "common-command" || path.parts.last().is_some_and(|part| part.kind == "command")
}

/// Recognize configuration payloads exported below the root `Ext` folder.
fn is_configuration_artifact(components: &[&str]) -> bool {
    matches!(
        components,
        ["MobileClientSignature.bin" | "ParentConfigurations.bin"]
            | ["MainSectionPicture" | "Splash" | "ParentConfigurations", ..]
    )
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
        "DocumentNumerators" => "document-numerator",
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

/// Map a Designer XML object tag to its stable machine-facing metadata type.
pub fn kind_from_tag(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "Configuration" => "configuration",
        "ExternalDataProcessor" | "DataProcessor" => "data-processor",
        "ExternalReport" | "Report" => "report",
        "AccountingRegister" => "accounting-register",
        "AccumulationRegister" => "accumulation-register",
        "Bot" => "bot",
        "BusinessProcess" => "business-process",
        "CalculationRegister" => "calculation-register",
        "Catalog" => "catalog",
        "ChartOfAccounts" => "chart-of-accounts",
        "ChartOfCalculationTypes" => "chart-of-calculation-types",
        "ChartOfCharacteristicTypes" => "chart-of-characteristic-types",
        "CommandGroup" => "command-group",
        "CommonAttribute" => "common-attribute",
        "CommonCommand" => "common-command",
        "CommonForm" => "common-form",
        "CommonModule" => "common-module",
        "CommonPicture" => "common-picture",
        "CommonTemplate" => "common-template",
        "Constant" => "constant",
        "DefinedType" => "defined-type",
        "Document" => "document",
        "DocumentJournal" => "document-journal",
        "DocumentNumerator" => "document-numerator",
        "Enum" => "enum",
        "EventSubscription" => "event-subscription",
        "ExchangePlan" => "exchange-plan",
        "ExternalDataSource" => "external-data-source",
        "FilterCriterion" => "filter-criterion",
        "FunctionalOption" => "functional-option",
        "FunctionalOptionsParameter" => "functional-option-parameter",
        "HTTPService" => "http-service",
        "InformationRegister" => "information-register",
        "IntegrationService" => "integration-service",
        "Language" => "language",
        "Role" => "role",
        "ScheduledJob" => "scheduled-job",
        "Sequence" => "sequence",
        "SessionParameter" => "session-parameter",
        "SettingsStorage" => "settings-storage",
        "Style" => "style",
        "StyleItem" => "style-item",
        "Subsystem" => "subsystem",
        "Task" => "task",
        "WebService" => "web-service",
        "WSReference" => "ws-reference",
        "XDTOPackage" => "xdto-package",
        "Form" => "form",
        "Template" => "template",
        "Command" => "command",
        child => return child_kind(child),
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

    /// Designer payload files collapse into the same Configurator owner as their descriptor.
    #[test]
    fn resolves_owned_help_command_form_template_and_package_payloads() {
        for (descriptor, payload) in [
            (
                "CommonCommands/Refresh.xml",
                "CommonCommands/Refresh/Ext/CommandModule.bsl",
            ),
            (
                "CommonForms/Choice.xml",
                "CommonForms/Choice/Ext/Form/Module.bsl",
            ),
            (
                "CommonPictures/Logo.xml",
                "CommonPictures/Logo/Ext/Picture/Picture.svg",
            ),
            (
                "CommonTemplates/Help.xml",
                "CommonTemplates/Help/Ext/Template/ru.html",
            ),
            (
                "Reports/Sales/Templates/Layout.xml",
                "Reports/Sales/Templates/Layout/Ext/Template/Items/Logo/Picture.png",
            ),
            (
                "Reports/Sales/Forms/Main.xml",
                "Reports/Sales/Forms/Main/Ext/Help/_files/example.png",
            ),
            (
                "XDTOPackages/Exchange.xml",
                "XDTOPackages/Exchange/Ext/Package.bin",
            ),
            (
                "WSReferences/Statistics.xml",
                "WSReferences/Statistics/Ext/1.xsd",
            ),
        ] {
            assert_eq!(
                resolve(descriptor),
                resolve(payload),
                "payload `{payload}` must resolve to `{descriptor}`"
            );
        }
        assert_eq!(
            resolve("DataProcessors/Import/Ext/ObjectModule.bsl"),
            resolve("DataProcessors/Import/Ext/ObjectModule.bin")
        );
    }

    /// Recursive subsystems, numerators and root payloads retain their nearest logical owner.
    #[test]
    fn resolves_nested_subsystems_numerators_and_configuration_payloads() {
        let subsystem = resolve("Subsystems/Accounting/Subsystems/Taxes.xml");
        assert_eq!(subsystem.parts.len(), 2);
        assert_eq!(subsystem.parts[1].kind, "subsystem");
        assert_eq!(subsystem.parts[1].name.as_deref(), Some("Taxes"));
        assert_eq!(
            subsystem,
            resolve("Subsystems/Accounting/Subsystems/Taxes/Ext/CommandInterface.xml")
        );
        assert_eq!(
            resolve("Configuration.xml"),
            resolve("Ext/ParentConfigurations/Base.cf")
        );
        assert_eq!(
            resolve("Configuration.xml"),
            resolve("Ext/MainSectionPicture/Picture.svg")
        );

        let numerator = resolve("DocumentNumerators/Invoices.xml");
        assert_eq!(numerator.group, "document-numerator");
        assert_eq!(numerator.parts[0].name.as_deref(), Some("Invoices"));
        assert!(
            from_path(
                ProjectType::Configuration,
                b"Catalogs/Partners/Ext/unknown.dat".as_bstr()
            )
            .is_none()
        );
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

    /// Resolve one UTF-8 fixture path for concise ownership comparisons.
    fn resolve(path: &str) -> super::MetadataPath {
        from_path(ProjectType::Configuration, path.as_bytes().as_bstr()).unwrap()
    }
}
