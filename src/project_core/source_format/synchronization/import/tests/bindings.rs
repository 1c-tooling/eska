//! Current editor targets derive from validated composition while saved plans fence later source changes.

use super::*;
use crate::project_core::source_format::{
    Operation, PropertyName, PropertyValue, prepare_module_edit, prepare_property_edit,
};

/// Reopen the original independently pinned store before deriving a current source view.
fn bound(fixture: &Fixture, source: &std::path::Path) -> AcceptedBase {
    AcceptedBase::open(
        fixture.approved.storage_root(),
        fixture.approved.reference(),
    )
    .unwrap()
    .bind_source(source)
    .unwrap()
}

/// Renamed paths remain editable under the old UUID/pin; exact source bytes and properties are retained.
#[test]
fn renamed_source_view_supports_saved_module_and_property_edits() {
    let fixture = Fixture::new(false);
    rename_incoming(&fixture, "Helpers");
    let incoming = fixture.incoming();
    let output = fixture.test.0.join("output");
    let plan = prepare_source_import(
        &fixture.source,
        &fixture.ancestor,
        &incoming,
        &fixture.approved,
    )
    .unwrap();
    assert!(
        plan.support_permissions()
            .iter()
            .any(
                |permission| permission.uuid.as_str() == "20000000-0000-4000-8000-000000000001"
                    && permission.allowed
            )
    );
    plan.publish_source(&output).unwrap();
    let authority = bound(&fixture, &output);
    let module = SourcePath::parse("CommonModules/Helpers/Module.bsl").unwrap();
    let permission = authority.module_permission(&module).unwrap();
    assert!(permission.allowed);
    assert_eq!(permission.baseline, fixture.approved.reference().sha256);
    assert!(
        authority
            .module_permission(&SourcePath::parse(MODULE).unwrap())
            .is_err()
    );
    let original = fs::read_to_string(output.join(module.as_path())).unwrap();
    let plan = prepare_module_edit(
        &output,
        &authority,
        &module,
        Some(&original),
        "\u{feff}// edited\r\n",
    )
    .unwrap();
    plan.publish(&authority).unwrap();
    assert_eq!(
        fs::read(output.join(module.as_path())).unwrap(),
        b"\xef\xbb\xbf// edited\r\n"
    );
    let property = prepare_property_edit(
        &output,
        &authority,
        &permission.uuid,
        PropertyName::Server,
        PropertyValue::Boolean(false),
    )
    .unwrap();
    property.publish(&authority).unwrap();
    assert!(
        fs::read_to_string(output.join("CommonModules/Helpers/object.yaml"))
            .unwrap()
            .contains("\"Server\": false")
    );
    let rebound = authority.bind_source(&output).unwrap();
    assert!(
        rebound
            .permission(&permission.uuid, Operation::Module)
            .allowed
    );
    rebound.verify().unwrap();
}

/// Complete new owners gain only validated parent-derived authority; the original approval is unchanged.
#[test]
fn created_owner_view_supports_edits_without_accepting_new_source_bytes() {
    let fixture = Fixture::new(false);
    add_incoming(&fixture, false);
    let incoming = fixture.incoming();
    let output = fixture.test.0.join("output");
    let plan = prepare_source_import(
        &fixture.source,
        &fixture.ancestor,
        &incoming,
        &fixture.approved,
    )
    .unwrap();
    assert!(
        plan.support_permissions()
            .iter()
            .any(
                |permission| permission.uuid.as_str() == "30000000-0000-4000-8000-000000000001"
                    && permission.allowed
            )
    );
    plan.publish_source(&output).unwrap();
    let authority = bound(&fixture, &output);
    let uuid = NativeUuid::parse("30000000-0000-4000-8000-000000000001").unwrap();
    assert!(
        !fixture
            .approved
            .permission(&uuid, Operation::Module)
            .allowed
    );
    assert!(authority.permission(&uuid, Operation::Module).allowed);
    let property = prepare_property_edit(
        &output,
        &authority,
        &uuid,
        PropertyName::Server,
        PropertyValue::Boolean(false),
    )
    .unwrap();
    property.publish(&authority).unwrap();
    let authority = authority.bind_source(&output).unwrap();
    prepare_protected(
        &output,
        &authority,
        &fixture.test.0.join("generations"),
        true,
    )
    .unwrap();
    fixture.approved.verify().unwrap();
}

/// A publication plan keeps its captured fence even if a caller supplies a freshly rebound handle.
#[test]
fn source_structure_change_invalidates_a_saved_editor_plan_before_writing() {
    let fixture = Fixture::new(false);
    let authority = bound(&fixture, &fixture.source);
    let module = SourcePath::parse(MODULE).unwrap();
    let original = fs::read_to_string(fixture.source.join(MODULE)).unwrap();
    let plan = prepare_module_edit(
        &fixture.source,
        &authority,
        &module,
        Some(&original),
        "// unwanted\r\n",
    )
    .unwrap();
    let path = fixture.source.join("configuration.yaml");
    let contents = fs::read_to_string(&path).unwrap();
    fs::write(path, format!("{contents}# changed after preview\n")).unwrap();
    let replacement = bound(&fixture, &fixture.source);
    assert_eq!(
        plan.publish(&replacement).unwrap_err().code,
        "source_edit_structure_changed"
    );
    assert_eq!(
        fs::read_to_string(fixture.source.join(MODULE)).unwrap(),
        original
    );
}

/// Frozen structures, root identity changes, aliases and cancellation never yield a partial view.
#[test]
fn source_binding_rejects_invalid_or_cancelled_composition() {
    let fixture = Fixture::new(true);
    rename_incoming(&fixture, "Helpers");
    let authority = AcceptedBase::open(
        fixture.approved.storage_root(),
        fixture.approved.reference(),
    )
    .unwrap();
    assert_eq!(
        authority
            .bind_source(&fixture.incoming_source)
            .err()
            .unwrap()
            .code,
        "source_import_structure_denied"
    );
    let authority = AcceptedBase::open(
        fixture.approved.storage_root(),
        fixture.approved.reference(),
    )
    .unwrap();
    assert_eq!(
        authority
            .bind_source_cancellable(&fixture.source, &|| true)
            .err()
            .unwrap()
            .code,
        "source_read_cancelled"
    );
    #[cfg(unix)]
    {
        let alias = fixture.test.0.join("alias");
        std::os::unix::fs::symlink(&fixture.source, &alias).unwrap();
        let authority = AcceptedBase::open(
            fixture.approved.storage_root(),
            fixture.approved.reference(),
        )
        .unwrap();
        assert_eq!(
            authority.bind_source(&alias).err().unwrap().code,
            "source_edit_unsafe_path"
        );
    }
    let path = fixture.source.join("configuration.yaml");
    let contents = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        contents.replace(
            "10000000-0000-4000-8000-000000000001",
            "90000000-0000-4000-8000-000000000001",
        ),
    )
    .unwrap();
    let authority = AcceptedBase::open(
        fixture.approved.storage_root(),
        fixture.approved.reference(),
    )
    .unwrap();
    assert_eq!(
        authority.bind_source(&fixture.source).err().unwrap().code,
        "source_identity_mismatch"
    );
}

/// New managed form properties and their plain queries use one checked owner and the old support pin.
#[test]
fn created_form_and_query_remain_editable_through_the_bound_source_view() {
    use crate::project_core::source_format::{
        FormEdit, FormPropertyName, FormPropertyValue, import_designer_queries, prepare_form_edit,
        prepare_query_edit,
        queries::test_support::{LIST, fixture as native_fixture},
    };
    let directory = TestDir::new();
    let native = native_fixture(&directory);
    let source = directory.0.join("source");
    import_designer_queries(&native, &source).unwrap();
    let store = directory.0.join("accepted");
    let reference = accept_source(&source, &store, &[]).unwrap();
    let ancestor_store = directory.0.join("ancestor");
    let ancestor_reference = capture_import_ancestor(&source, &ancestor_store).unwrap();
    let ancestor = ImportAncestor::open(&ancestor_store, &ancestor_reference).unwrap();
    super::additions::clone_owner(
        &native,
        "Catalogs/Sample/Forms/Main",
        "Catalogs/Sample/Forms/Second",
        "90000000-0000-4000-8000-000000000001",
    );
    let path = native.join("Catalogs/Sample.xml");
    let mut contents = fs::read_to_string(&path).unwrap();
    let index = contents.rfind("</ChildObjects>").unwrap();
    contents.insert_str(index, "<Form>Second</Form>");
    fs::write(path, contents).unwrap();
    let path = native.join("Catalogs/Sample/Forms/Second/Ext/Form.xml");
    let contents = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        contents.replace("</Form>", "<ReadOnly>true</ReadOnly></Form>"),
    )
    .unwrap();
    let incoming_source = directory.0.join("incoming-source");
    import_designer_queries(&native, &incoming_source).unwrap();
    let incoming_store = directory.0.join("incoming");
    let incoming_reference = capture_import_ancestor(&incoming_source, &incoming_store).unwrap();
    let incoming = ImportAncestor::open(&incoming_store, &incoming_reference).unwrap();
    let approved = AcceptedBase::open(&store, &reference).unwrap();
    let output = directory.0.join("output");
    prepare_source_import(&source, &ancestor, &incoming, &approved)
        .unwrap()
        .publish_source(&output)
        .unwrap();
    let authority = AcceptedBase::open(&store, &reference)
        .unwrap()
        .bind_source(&output)
        .unwrap();
    let uuid = NativeUuid::parse("90000000-0000-4000-8000-000000000001").unwrap();
    let plan = prepare_form_edit(
        &output,
        &authority,
        &uuid,
        FormEdit::Property {
            element: None,
            property: FormPropertyName::ReadOnly,
            value: FormPropertyValue::Bool(false),
        },
    )
    .unwrap();
    plan.publish(&authority).unwrap();
    let authority = authority.bind_source(&output).unwrap();
    let query = SourcePath::parse(&LIST.replace("/Main/", "/Second/")).unwrap();
    let plan = prepare_query_edit(&output, &authority, &query, "SELECT 42;\r\n").unwrap();
    plan.publish(&authority).unwrap();
    assert_eq!(
        fs::read_to_string(output.join(query.as_path())).unwrap(),
        "SELECT 42;\r\n"
    );
    let prepared =
        prepare_protected(&output, &approved, &directory.0.join("generations"), true).unwrap();
    let native = fs::read_to_string(
        prepared
            .xml()
            .join("Catalogs/Sample/Forms/Second/Ext/Form.xml"),
    )
    .unwrap();
    assert!(native.contains("SELECT 42;") && native.contains("<ReadOnly>false</ReadOnly>"));
    approved.verify().unwrap();
}
