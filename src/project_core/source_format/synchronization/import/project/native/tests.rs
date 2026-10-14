//! Native preparation never changes source/support before its reviewed project plan is applied.

use super::super::tests::Fixture;
use super::*;
use crate::{
    project_core::{
        config::ProjectConfig,
        discovery::discover,
        source_format::{LayoutPublication, SourceReader, accept_source, import_designer_queries},
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
