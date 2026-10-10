//! Register fields and filter dimensions require typed receivers and exact current parent identities.

use super::*;
use crate::session::tests::{draft, fixture};
use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{NativeUuid, import_designer},
};

#[path = "../../../tests/definition_register_queries/fixture.rs"]
mod registers;

const CALLER: &str = "Catalogs/Sample/Ext/ObjectModule.bsl";

/// Extend a private native copy and import into a fresh source directory under its owned fixture.
fn session() -> (
    crate::session::tests::support::TestDirectory,
    AnalysisSession,
) {
    let (directory, _) = fixture();
    registers::extend(&directory.0.join("native"));
    let source = directory.0.join("register-source");
    import_designer(&directory.0.join("native"), &source).unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    for case in &registers::CASES {
        let owner =
            SourcePath::parse(&format!("{}/{}/object.yaml", case.folder, case.name)).unwrap();
        session
            .load_saved(&owner, session.revision(), &InputRequest::default())
            .unwrap();
    }
    (directory, session)
}

/// Select the last complete fixture field token; BOM and non-BMP comments remain in current text.
fn query(
    session: &mut AnalysisSession,
    body: &str,
    field: &str,
    version: i64,
) -> Option<MetadataLocation> {
    let caller = SourcePath::parse(CALLER).unwrap();
    session
        .apply_module_overlay(
            &caller,
            session.revision(),
            version,
            body,
            &InputRequest::default(),
        )
        .unwrap();
    let offset = u32::try_from(body.rfind(field).unwrap() + 1).unwrap();
    session
        .module_metadata_definition(
            &caller,
            session.revision(),
            offset,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
}

/// Every register flavour resolves record fields and filter dimensions to its own exact child UUID.
#[test]
fn register_records_and_filter_dimensions_navigate_the_declared_current_members() {
    let (_directory, mut session) = session();
    let mut version = 1;
    for case in &registers::CASES {
        let owner =
            SourcePath::parse(&format!("{}/{}/object.yaml", case.folder, case.name)).unwrap();
        for (receiver, field, part) in [
            ("Record=Set.Add();", "kEY", 1),
            ("Record=Set.Add();", "aMOUNT", 2),
            ("Record=Set.Add();", "nOTE", 3),
            ("Record=Set.Filter;", "kEY", 1),
        ] {
            let body = registers::body(case, receiver, field);
            let target = query(&mut session, &body, field, version).unwrap();
            version += 1;
            let uuid = NativeUuid::parse(&registers::uuid(case, part)).unwrap();
            assert_eq!(target.source, owner);
            assert_eq!(target.uuid, uuid.as_str());
            assert_eq!(
                target,
                session
                    .metadata_location(&owner, session.revision(), &uuid, None)
                    .unwrap()
            );
        }
    }
    let case = &registers::CASES[0];
    for (field, part) in [("kEY", 1), ("aMOUNT", 2), ("nOTE", 3)] {
        let body = registers::body(
            case,
            "Record=Collection.Rates.CreateRecordManager();",
            field,
        );
        assert_eq!(
            query(&mut session, &body, field, version).unwrap().uuid,
            registers::uuid(case, part)
        );
        version += 1;
    }
}

/// Standard properties, non-dimension filters and fields on the record set do not fabricate targets.
#[test]
fn register_member_navigation_refuses_receivers_without_a_declared_field() {
    let (_directory, mut session) = session();
    let mut version = 1;
    for case in &registers::CASES {
        for (receiver, field) in [
            ("Record=Set;", "Key"),
            ("Record=Set.Filter;", "Amount"),
            ("Record=Set.Filter;", "Note"),
            ("Record=Set.Add();", "Period"),
            ("Record=Set.Filter;", "Period"),
            ("Record=New Structure(\"Key\",1);", "Key"),
        ] {
            let body = registers::body(case, receiver, field);
            assert!(
                query(&mut session, &body, field, version).is_none(),
                "{}: {body}",
                case.kind
            );
            version += 1;
        }
    }
}

/// The canonical register and filter facade stop confirming an old dimension immediately after rename.
#[test]
fn register_dimension_rename_updates_record_and_filter_targets_without_old_model_fallback() {
    let (_directory, mut session) = session();
    let case = &registers::CASES[0];
    let owner = SourcePath::parse("InformationRegisters/Rates/object.yaml").unwrap();
    let body = registers::body(case, "Record=Set.Filter;", "Key");
    let first = query(&mut session, &body, "Key", 1).unwrap();
    let draft = draft(session.root(), &owner, "Key", "ChangedKey");
    session
        .apply_overlay(
            &owner,
            session.revision(),
            7,
            &draft,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(query(&mut session, &body, "Key", 2).is_none());
    let current = body.replace("Record.Key", "Record.ChangedKey");
    let renamed = query(&mut session, &current, "ChangedKey", 3).unwrap();
    assert_eq!(renamed.uuid, first.uuid);
    assert_ne!(renamed.source_sha256, first.source_sha256);
    session
        .apply_overlay(
            &owner,
            session.revision(),
            8,
            "broken: [\n",
            &InputRequest::default(),
        )
        .unwrap();
    assert!(query(&mut session, &current, "ChangedKey", 4).is_none());
    session
        .close_overlay(&owner, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(query(&mut session, &current, "ChangedKey", 5).is_none());
    let restored = query(&mut session, &body, "Key", 6).unwrap();
    assert_eq!(restored.revision, session.revision());
    assert_ne!(restored.revision, first.revision);
    assert_eq!(
        restored,
        MetadataLocation {
            revision: restored.revision,
            ..first
        }
    );
}
