//! Native preparation never changes source/support before its reviewed project plan is applied.

use super::super::tests::Fixture;
use super::*;
use crate::{
    project_core::{
        config::ProjectConfig,
        discovery::discover,
        source_format::{
            AcceptedBase, LayoutPublication, SourceReader, accept_source, import_designer_queries,
            import_designer_readable, prepare_protected, project_import_ancestor,
        },
    },
    test_support::TestDir,
};

/// Ordinary native XML keeps the old readable map and preserves an independent local module.
#[test]
fn native_projection_uses_current_layout_and_applies_independent_changes() {
    let fixture = Fixture::new(false);
    let native = fixture.test.0.join("native");
    let path = native.join("Configuration.xml");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap().replace("1.0.0", "1.0.1"),
    )
    .unwrap();
    let before = Inventory::read(fixture.project.root()).unwrap();
    let staging = fixture.test.0.join("projection");
    let prepared = prepare_project_native_source_import(
        &fixture.project,
        &ProjectScope::Standalone,
        &fixture.approved,
        &fixture.ancestor,
        &native,
        &staging,
        &|| false,
    )
    .unwrap();
    assert!(prepared.plan().source_plan().conflicts().is_empty());
    assert_eq!(Inventory::read(fixture.project.root()).unwrap(), before);
    assert_eq!(
        prepared.plan().publish().unwrap(),
        LayoutPublication::Created
    );
    assert_eq!(
        prepared.plan().publish().unwrap(),
        LayoutPublication::Unchanged
    );
    assert!(
        fs::read_to_string(fixture.project.source().join("configuration.yaml"))
            .unwrap()
            .contains("1.0.1")
    );
    assert_eq!(
        fs::read(
            fixture
                .project
                .source()
                .join("CommonModules/Tools/Module.bsl")
        )
        .unwrap(),
        b"\xef\xbb\xbf// local survives\r\n"
    );
    drop(prepared);
    assert!(!staging.exists());
    assert!(fixture.project.root().join(super::super::SERVICE).exists());
}

/// Native adoption belongs to the direct MD Properties slot, separately from the descriptor UUID.
fn assert_native_adoption(
    path: &std::path::Path,
    kind: &str,
    descriptor_uuid: &str,
    belonging: &str,
    extended_uuid: Option<&str>,
) {
    const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
    let text = fs::read_to_string(path).unwrap();
    let document = roxmltree::Document::parse(&text).unwrap();
    let root = document.root_element();
    assert!(root.has_tag_name((MD, "MetaDataObject")));
    let objects: Vec<_> = root
        .children()
        .filter(|node| node.has_tag_name((MD, kind)))
        .collect();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].attribute("uuid"), Some(descriptor_uuid));
    let properties: Vec<_> = objects[0]
        .children()
        .filter(|node| node.has_tag_name((MD, "Properties")))
        .collect();
    assert_eq!(properties.len(), 1);
    let adoption: Vec<_> = properties[0]
        .children()
        .filter(|node| node.has_tag_name((MD, "ObjectBelonging")))
        .map(|node| node.text())
        .collect();
    assert_eq!(adoption, [Some(belonging)]);
    let extended: Vec<_> = properties[0]
        .children()
        .filter(|node| node.has_tag_name((MD, "ExtendedConfigurationObject")))
        .map(|node| node.text())
        .collect();
    assert_eq!(
        extended,
        extended_uuid.into_iter().map(Some).collect::<Vec<_>>()
    );
    if let Some(extended_uuid) = extended_uuid {
        assert_ne!(descriptor_uuid, extended_uuid);
    }
}

const BORROWED_CFE_ROOT_UUID: &str = "10000000-0000-4000-8000-000000000001";
const BORROWED_CFE_TOOLS_UUID: &str = "20000000-0000-4000-8000-000000000001";
const BORROWED_CFE_DESCRIPTOR_UUID: &str = "30000000-0000-4000-8000-000000000001";
const BORROWED_CFE_TARGET_UUID: &str = "90000000-0000-4000-8000-000000000001";
const BORROWED_CFE_LOCAL_PATH: &str = "CommonModules/Tools/Module.bsl";
const BORROWED_CFE_LOCAL_BYTES: &[u8] = b"\xef\xbb\xbf// independent local Tools\r\n";
const BORROWED_CFE_MODULE_BYTES: &[u8] = b"\xef\xbb\xbf// borrowed native module\r\n";

/// CFE native identity and exact settings are established before support approval or ancestry.
fn borrowed_extension_project(test: &TestDir) -> std::path::PathBuf {
    let initial_source = crate::project_core::source_format::synchronization::tests::source(test);
    let native = test.0.join("native");
    let root_xml = native.join("Configuration.xml");
    let root_text = fs::read_to_string(&root_xml).unwrap().replace(
        "<Properties>",
        concat!(
            "<Properties><ObjectBelonging>Adopted</ObjectBelonging>",
            "<ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>"
        ),
    );
    fs::write(&root_xml, root_text).unwrap();
    let tools_xml = native.join("CommonModules/Tools.xml");
    let tools_text = fs::read_to_string(&tools_xml).unwrap().replace(
        "<Properties>",
        "<Properties><ObjectBelonging>Native</ObjectBelonging>",
    );
    fs::write(&tools_xml, tools_text).unwrap();
    assert_native_adoption(
        &root_xml,
        "Configuration",
        BORROWED_CFE_ROOT_UUID,
        "Adopted",
        None,
    );
    assert_native_adoption(
        &tools_xml,
        "CommonModule",
        BORROWED_CFE_TOOLS_UUID,
        "Native",
        None,
    );
    fs::remove_dir_all(initial_source).unwrap();
    let root = test.0.join("project");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join(".eska")).unwrap();
    let source = root.join("src");
    import_designer_readable(&native, &source).unwrap();
    let reader = SourceReader::open(&source).unwrap();
    assert_eq!(
        reader.project_type().unwrap(),
        crate::project_core::ProjectType::Extension
    );
    let config = ProjectConfig::new(reader.project_type().unwrap())
        .with_source_format(SourceFormat::Eska {
            accepted_base: SourcePath::parse(".eska/accepted").unwrap(),
        })
        .unwrap();
    fs::write(root.join("eska.toml"), config.to_toml().unwrap()).unwrap();
    fs::write(root.join(".gitattributes"), "*.bsl text eol=crlf\r\n").unwrap();
    fs::write(root.join(".gitignore"), "build/\r\n").unwrap();
    root
}

/// Incoming borrowed metadata keeps its own descriptor UUID and the distinct native target UUID.
fn add_borrowed_native_module(native: &std::path::Path) {
    let root_xml = native.join("Configuration.xml");
    fs::write(
        &root_xml,
        fs::read_to_string(&root_xml).unwrap().replace(
            "</ChildObjects>",
            "<CommonModule>Borrowed</CommonModule></ChildObjects>",
        ),
    )
    .unwrap();
    fs::create_dir_all(native.join("CommonModules/Borrowed/Ext")).unwrap();
    fs::write(
        native.join("CommonModules/Borrowed.xml"),
        format!(
            concat!(
                "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'>",
                "<CommonModule uuid='{borrowed_uuid}'><Properties>",
                "<ObjectBelonging>Adopted</ObjectBelonging>",
                "<ExtendedConfigurationObject>{extended_uuid}</ExtendedConfigurationObject>",
                "<Name>Borrowed</Name><Server>true</Server>",
                "</Properties></CommonModule></MetaDataObject>"
            ),
            borrowed_uuid = BORROWED_CFE_DESCRIPTOR_UUID,
            extended_uuid = BORROWED_CFE_TARGET_UUID,
        ),
    )
    .unwrap();
    fs::write(
        native.join("CommonModules/Borrowed/Ext/Module.bsl"),
        BORROWED_CFE_MODULE_BYTES,
    )
    .unwrap();
}

/// Both borrowed files are incoming additions, never accepted or local replacements.
fn assert_borrowed_addition_plan(plan: &ProjectSourceImportPlan, pin: &AcceptedRef) {
    assert!(
        plan.source_plan().conflicts().is_empty(),
        "{:?}",
        plan.source_plan().conflicts()
    );
    assert_eq!(plan.source_plan().approved_reference(), pin);
    for path in [
        "CommonModules/Borrowed/object.yaml",
        "CommonModules/Borrowed/Module.bsl",
    ] {
        let change = plan
            .source_plan()
            .changes()
            .iter()
            .find(|change| {
                change
                    .incoming
                    .as_ref()
                    .is_some_and(|entry| entry.path.as_str() == path)
            })
            .unwrap();
        assert!(change.ancestor.is_none());
        assert!(change.working.is_none());
        assert!(change.incoming.is_some());
        assert!(change.selected.is_some());
        assert_eq!(change.selected, change.incoming);
    }
}

/// Strict export uses the old approval while preserving native adoption and exact module bytes.
fn assert_borrowed_strict_output(
    test: &TestDir,
    source: &std::path::Path,
    base: &AcceptedBase,
    pin: &AcceptedRef,
) {
    let reader = SourceReader::open(source).unwrap();
    let borrowed = reader
        .read_owner(&SourcePath::parse("CommonModules/Borrowed/object.yaml").unwrap())
        .unwrap();
    assert_eq!(
        borrowed.source.document.descriptor.object.uuid.as_str(),
        BORROWED_CFE_DESCRIPTOR_UUID
    );
    let strict = prepare_protected(source, base, &test.0.join("strict"), true).unwrap();
    assert_eq!(strict.baseline(), pin.sha256);
    assert!(strict.diagnostics().is_empty());
    strict.validate().unwrap();
    assert_native_adoption(
        &strict.xml().join("CommonModules/Borrowed.xml"),
        "CommonModule",
        BORROWED_CFE_DESCRIPTOR_UUID,
        "Adopted",
        Some(BORROWED_CFE_TARGET_UUID),
    );
    assert_native_adoption(
        &strict.xml().join("CommonModules/Tools.xml"),
        "CommonModule",
        BORROWED_CFE_TOOLS_UUID,
        "Native",
        None,
    );
    assert_eq!(
        fs::read(source.join(BORROWED_CFE_LOCAL_PATH)).unwrap(),
        BORROWED_CFE_LOCAL_BYTES
    );
    assert_eq!(
        fs::read(strict.xml().join("CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        BORROWED_CFE_LOCAL_BYTES
    );
    assert_eq!(
        fs::read(source.join("CommonModules/Borrowed/Module.bsl")).unwrap(),
        BORROWED_CFE_MODULE_BYTES
    );
    assert_eq!(
        fs::read(strict.xml().join("CommonModules/Borrowed/Ext/Module.bsl")).unwrap(),
        BORROWED_CFE_MODULE_BYTES
    );
}

/// A native borrowed CFE addition keeps adoption and target identity under the original approval.
#[test]
fn borrowed_extension_addition_preserves_native_adoption_and_local_module() {
    let test = TestDir::new();
    let root = borrowed_extension_project(&test);
    let native = test.0.join("native");
    let source = root.join("src");
    let settings = ["eska.toml", ".gitattributes", ".gitignore"]
        .map(|name| (name, fs::read(root.join(name)).unwrap()));
    let accepted_root = root.join(".eska/accepted");
    let pin = accept_source(&source, &accepted_root, &[]).unwrap();
    let base = AcceptedBase::open(&accepted_root, &pin).unwrap();
    let accepted_before = Inventory::read(&accepted_root).unwrap();
    let store = test.0.join("ancestor");
    let ancestor_pin = capture_import_ancestor(&source, &store).unwrap();
    let ancestor = ImportAncestor::open(&store, &ancestor_pin).unwrap();
    let ancestor_before = Inventory::read(&store).unwrap();
    let project = discover(&root).unwrap();
    fs::write(
        source.join(BORROWED_CFE_LOCAL_PATH),
        BORROWED_CFE_LOCAL_BYTES,
    )
    .unwrap();
    add_borrowed_native_module(&native);
    let native_before = Inventory::read(&native).unwrap();
    let before = Inventory::read(&root).unwrap();
    let staging = test.0.join("projection");
    let prepared = prepare_project_native_source_import(
        &project,
        &ProjectScope::Standalone,
        &pin,
        &ancestor,
        &native,
        &staging,
        &|| false,
    )
    .unwrap();
    assert_borrowed_addition_plan(prepared.plan(), &pin);
    assert_eq!(Inventory::read(&root).unwrap(), before);
    assert_eq!(Inventory::read(&native).unwrap(), native_before);
    assert_eq!(Inventory::read(&store).unwrap(), ancestor_before);
    assert_eq!(
        prepared.plan().publish().unwrap(),
        LayoutPublication::Created
    );
    let incoming_pin = prepared.plan().source_plan().incoming_reference().clone();
    assert_eq!(
        prepared.plan().resulting_ancestor_reference(),
        &incoming_pin
    );
    drop(prepared);
    assert!(!staging.exists());
    assert_borrowed_strict_output(&test, &source, &base, &pin);
    let saved =
        project_import_ancestor(&project, &ProjectScope::Standalone, &incoming_pin).unwrap();
    let raw_incoming = test.0.join("raw-incoming");
    saved.restore(&raw_incoming).unwrap();
    assert_eq!(
        fs::read(raw_incoming.join(BORROWED_CFE_LOCAL_PATH)).unwrap(),
        b"\xef\xbb\xbf// original\r\n"
    );
    let before_repeat = Inventory::read(&root).unwrap();
    let repeat_staging = test.0.join("repeat-projection");
    let repeated = prepare_project_native_source_import(
        &project,
        &ProjectScope::Standalone,
        &pin,
        &saved,
        &native,
        &repeat_staging,
        &|| false,
    )
    .unwrap();
    assert!(repeated.plan().source_plan().conflicts().is_empty());
    assert_eq!(repeated.plan().source_plan().approved_reference(), &pin);
    assert_eq!(
        repeated.plan().resulting_ancestor_reference(),
        saved.reference()
    );
    assert_eq!(
        repeated.plan().publish().unwrap(),
        LayoutPublication::Unchanged
    );
    drop(repeated);
    assert!(!repeat_staging.exists());
    assert_eq!(Inventory::read(&root).unwrap(), before_repeat);
    assert_eq!(Inventory::read(&accepted_root).unwrap(), accepted_before);
    assert_eq!(Inventory::read(&store).unwrap(), ancestor_before);
    assert_eq!(Inventory::read(&native).unwrap(), native_before);
    for (name, bytes) in settings {
        assert_eq!(fs::read(root.join(name)).unwrap(), bytes);
    }
}

/// EPF/ERF filenames and their whole prefix change without changing any source binding or payload.
#[test]
fn external_artifact_stems_rebind_to_the_approved_native_root() {
    for tag in ["ExternalDataProcessor", "ExternalReport"] {
        let test = TestDir::new();
        let native = test.0.join("native");
        fs::create_dir_all(native.join("Original.v1/Ext")).unwrap();
        fs::create_dir(native.join("Original.v1/Empty")).unwrap();
        fs::write(native.join("Original.v1.xml"), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><{tag} uuid='10000000-0000-4000-8000-000000000001'><Properties><Name>SameIdentity</Name><Comment>Original</Comment></Properties></{tag}></MetaDataObject>")).unwrap();
        fs::write(
            native.join("Original.v1/Ext/ObjectModule.bsl"),
            b"\xef\xbb\xbf// original\r\n",
        )
        .unwrap();
        fs::write(native.join("Original.v1/Ext/opaque.bin"), [0, 255, 10, 13]).unwrap();
        let project = test.0.join("project");
        fs::create_dir(&project).unwrap();
        fs::create_dir(project.join(".eska")).unwrap();
        let source = project.join("src");
        import_designer_queries(&native, &source).unwrap();
        let expected = accept_source(&source, &project.join(".eska/accepted"), &[]).unwrap();
        let reader = SourceReader::open(&source).unwrap();
        let config = ProjectConfig::new(reader.project_type().unwrap())
            .with_source_format(SourceFormat::Eska {
                accepted_base: SourcePath::parse(".eska/accepted").unwrap(),
            })
            .unwrap();
        fs::write(project.join("eska.toml"), config.to_toml().unwrap()).unwrap();
        let project = discover(&project).unwrap();
        let store = test.0.join("ancestor");
        let pin = capture_import_ancestor(&source, &store).unwrap();
        let ancestor = ImportAncestor::open(&store, &pin).unwrap();
        fs::write(source.join("ObjectModule.bsl"), "// local\r\n").unwrap();
        fs::rename(
            native.join("Original.v1.xml"),
            native.join("Другая.версия.v2.xml"),
        )
        .unwrap();
        fs::rename(native.join("Original.v1"), native.join("Другая.версия.v2")).unwrap();
        let path = native.join("Другая.версия.v2.xml");
        fs::write(
            &path,
            fs::read_to_string(&path)
                .unwrap()
                .replace("<Comment>Original</Comment>", "<Comment>Incoming</Comment>"),
        )
        .unwrap();
        let prepared = prepare_project_native_source_import(
            &project,
            &ProjectScope::Standalone,
            &expected,
            &ancestor,
            &native,
            &test.0.join("projection"),
            &|| false,
        )
        .unwrap();
        assert!(
            prepared.plan().source_plan().conflicts().is_empty(),
            "{:?}",
            prepared.plan().source_plan().conflicts()
        );
        assert_eq!(
            prepared.plan().publish().unwrap(),
            LayoutPublication::Created
        );
        assert_eq!(
            fs::read(source.join("ObjectModule.bsl")).unwrap(),
            b"// local\r\n"
        );
        assert!(!native.join("Original.v1.xml").exists());
        assert_eq!(
            fs::read(native.join("Другая.версия.v2/Ext/opaque.bin")).unwrap(),
            [0, 255, 10, 13]
        );
    }
}

/// Wrong UUID/type, unsafe overlap and cancellation refuse before a new projection is claimed.
#[test]
fn refused_native_inputs_do_not_write_projection_or_project() {
    for mode in ["uuid", "type", "broken", "cancel"] {
        let fixture = Fixture::new(false);
        let native = fixture.test.0.join("native");
        let path = native.join("Configuration.xml");
        let xml = fs::read_to_string(&path).unwrap();
        let changed = match mode {
            "uuid" => xml.replace(
                "10000000-0000-4000-8000-000000000001",
                "90000000-0000-4000-8000-000000000001",
            ),
            "type" => xml.replace(
                "<Version>",
                "<ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose><Version>",
            ),
            "broken" => "<MetaDataObject>".into(),
            _ => xml,
        };
        fs::write(&path, changed).unwrap();
        let before = Inventory::read(fixture.project.root()).unwrap();
        let staging = fixture.test.0.join("projection");
        assert!(
            prepare_project_native_source_import(
                &fixture.project,
                &ProjectScope::Standalone,
                &fixture.approved,
                &fixture.ancestor,
                &native,
                &staging,
                &|| mode == "cancel"
            )
            .is_err()
        );
        assert!(!staging.exists());
        assert_eq!(Inventory::read(fixture.project.root()).unwrap(), before);
    }
}

/// Even direct access to the project plan cannot bypass verification of its original native input.
#[test]
fn changed_native_input_blocks_plan_publication_and_preserves_existing_staging() {
    let fixture = Fixture::new(false);
    let native = fixture.test.0.join("native");
    let staging = fixture.test.0.join("projection");
    let prepared = prepare_project_native_source_import(
        &fixture.project,
        &ProjectScope::Standalone,
        &fixture.approved,
        &fixture.ancestor,
        &native,
        &staging,
        &|| false,
    )
    .unwrap();
    fs::write(native.join("Payload.bin"), "changed native input").unwrap();
    let before = Inventory::read(fixture.project.root()).unwrap();
    assert_eq!(
        prepared.plan().publish().unwrap_err().code,
        "source_input_changed"
    );
    assert_eq!(Inventory::read(fixture.project.root()).unwrap(), before);
    drop(prepared);
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("keep.txt"), "user data").unwrap();
    assert_eq!(
        prepare_project_native_source_import(
            &fixture.project,
            &ProjectScope::Standalone,
            &fixture.approved,
            &fixture.ancestor,
            &native,
            &staging,
            &|| false
        )
        .err()
        .unwrap()
        .code,
        "source_destination_exists"
    );
    assert_eq!(
        fs::read_to_string(staging.join("keep.txt")).unwrap(),
        "user data"
    );
}

/// XML presentation is ignored only where values, element order and qualified-name bindings agree.
#[test]
fn projection_comparison_preserves_xml_data_and_namespace_bindings() {
    let empty = "<ChildObjects xmlns='http://v8.1c.ru/8.3/MDClasses'/>";
    let indented = "<ChildObjects xmlns='http://v8.1c.ru/8.3/MDClasses'>\n\t</ChildObjects>";
    assert!(xml_equal(empty, indented, &|| false).unwrap());
    let original =
        "<Root xmlns:q='urn:one' value='same'><A> </A><B q:kind='q:Name'>text</B></Root>";
    let formatted = "<Root value=\"same\" xmlns:q=\"urn:one\">\n <A> </A>\n <B q:kind=\"q:Name\">text</B>\n</Root>";
    assert!(xml_equal(original, formatted, &|| false).unwrap());
    for changed in [
        formatted.replace("urn:one", "urn:other"),
        formatted.replace("text", "edited"),
        formatted.replace("<A> </A>", "<A></A>"),
        formatted.replace("q:Name", "q:Other"),
        formatted.replace(
            "<A> </A>\n <B q:kind=\"q:Name\">text</B>",
            "<B q:kind=\"q:Name\">text</B><A> </A>",
        ),
    ] {
        assert!(!xml_equal(original, &changed, &|| false).unwrap());
    }
    assert_eq!(
        xml_equal(original, formatted, &|| true).unwrap_err().code,
        "source_read_cancelled"
    );
}

/// Fresh dump versions remain incoming ancestry; they cannot replace pinned working bookkeeping.
#[test]
fn incoming_dump_bookkeeping_does_not_request_resource_permission_or_change_working_bytes() {
    let fixture = Fixture::new(false);
    let native = fixture.test.0.join("native");
    let source = fixture.project.source();
    let cdf = "<ConfigDumpInfo xmlns='http://v8.1c.ru/8.3/xcf/dumpinfo' format='Hierarchical' version='2.20'><ConfigVersions/></ConfigDumpInfo>";
    fs::write(native.join("ConfigDumpInfo.xml"), cdf).unwrap();
    let template = fixture.test.0.join("cdf-source");
    crate::project_core::source_format::import_designer_readable(&native, &template).unwrap();
    fs::remove_dir_all(source).unwrap();
    fs::rename(template, source).unwrap();
    fs::remove_dir_all(fixture.project.root().join(".eska/accepted")).unwrap();
    let expected =
        accept_source(source, &fixture.project.root().join(".eska/accepted"), &[]).unwrap();
    let store = fixture.test.0.join("cdf-ancestor");
    let reference = capture_import_ancestor(source, &store).unwrap();
    let ancestor = ImportAncestor::open(&store, &reference).unwrap();
    let incoming = cdf.replace("<ConfigVersions/>", "<ConfigVersions><Metadata name='Configuration.Sample' id='10000000-0000-4000-8000-000000000001' configVersion='changed'/></ConfigVersions>");
    fs::write(native.join("ConfigDumpInfo.xml"), &incoming).unwrap();
    let prepared = prepare_project_native_source_import(
        &fixture.project,
        &ProjectScope::Standalone,
        &expected,
        &ancestor,
        &native,
        &fixture.test.0.join("cdf-projection"),
        &|| false,
    )
    .unwrap();
    assert!(prepared.plan().source_plan().conflicts().is_empty());
    assert_eq!(
        prepared.plan().publish().unwrap(),
        LayoutPublication::Created
    );
    assert_eq!(
        fs::read_to_string(source.join("ConfigDumpInfo.xml")).unwrap(),
        cdf
    );
    let saved = crate::project_core::source_format::project_import_ancestor(
        &fixture.project,
        &ProjectScope::Standalone,
        prepared.plan().source_plan().incoming_reference(),
    )
    .unwrap();
    let restored = fixture.test.0.join("incoming-cdf");
    saved.restore(&restored).unwrap();
    assert_eq!(
        fs::read_to_string(restored.join("ConfigDumpInfo.xml")).unwrap(),
        incoming
    );
    drop(prepared);
    fs::write(
        native.join("ConfigDumpInfo.xml"),
        incoming.replace("changed", "next-dump"),
    )
    .unwrap();
    let before = Inventory::read(fixture.project.root()).unwrap();
    let repeated = prepare_project_native_source_import(
        &fixture.project,
        &ProjectScope::Standalone,
        &expected,
        &saved,
        &native,
        &fixture.test.0.join("repeat-cdf-projection"),
        &|| false,
    )
    .unwrap();
    assert!(repeated.plan().source_plan().conflicts().is_empty());
    assert_ne!(
        repeated.plan().source_plan().incoming_reference(),
        saved.reference()
    );
    assert_eq!(
        repeated.plan().resulting_ancestor_reference(),
        saved.reference()
    );
    assert_eq!(
        repeated.plan().publish().unwrap(),
        LayoutPublication::Unchanged
    );
    assert_eq!(
        repeated.plan().publish().unwrap(),
        LayoutPublication::Unchanged
    );
    assert_eq!(Inventory::read(fixture.project.root()).unwrap(), before);
    crate::project_core::source_format::project_import_ancestor(
        &fixture.project,
        &ProjectScope::Standalone,
        repeated.plan().resulting_ancestor_reference(),
    )
    .unwrap();
    fs::write(native.join("ConfigDumpInfo.xml"), "changed after preview").unwrap();
    assert_eq!(
        repeated.plan().publish().unwrap_err().code,
        "source_input_changed"
    );
    assert_eq!(Inventory::read(fixture.project.root()).unwrap(), before);
}
