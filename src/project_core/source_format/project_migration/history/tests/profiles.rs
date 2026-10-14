//! Four root types retain layout and support contracts even when no raw import history exists.

use crate::query_creation_fixture as fixture;

use super::*;
use crate::project_core::source_format::{
    NativeUuid, Operation, PermissionReason, SourceReader, export_designer,
};

/// Readable configurations migrate to Queries; existing external Queries remain map 6 without a bootstrap.
#[test]
fn history_absence_stays_absent_and_queries_migration_preserves_all_four_profiles() {
    for kind in fixture::TYPES {
        let test = TestDir::new();
        let native = fixture::native(&test.0, kind);
        let original = fixture::snapshot(&native.input);
        let external = matches!(kind, "processing" | "report");
        let import = if external {
            creation::import_query_project
        } else {
            creation::import_readable_project
        };
        let frozen = NativeUuid::parse(fixture::FORM_UUID).unwrap();
        let imported = import(
            &native.input,
            &test.0.join("old"),
            ProjectName::parse("sample".into()).unwrap(),
            None,
            std::slice::from_ref(&frozen),
        )
        .unwrap();
        if external {
            // Model an existing map 6 checkout without optional raw state; migration cannot invent it.
            fs::remove_dir_all(
                imported
                    .project
                    .root()
                    .join(PROJECT_SOURCE_IMPORT_DIRECTORY),
            )
            .unwrap();
        }
        fs::write(
            imported.project.source().join(native.module_source),
            b"\xef\xbb\xbf// local\r\n",
        )
        .unwrap();
        let plan = prepare_query_project(
            &imported.project,
            &ProjectScope::Standalone,
            &imported.accepted,
            &test.0.join("new"),
        )
        .unwrap();
        assert_eq!(plan.layout_changed(), !external);
        assert_eq!(plan.source_layout_version(), if external { 6 } else { 5 });
        assert!(!plan.ancestry_preserved());
        plan.publish().unwrap();
        assert!(
            !plan
                .destination()
                .join(PROJECT_SOURCE_IMPORT_DIRECTORY)
                .exists()
        );
        let copied = discover(plan.destination()).unwrap();
        let reader = SourceReader::open(copied.source()).unwrap();
        assert_eq!(reader.configuration().path.as_str(), native.root_source);
        assert_eq!(reader.query_paths().count(), 2);
        assert_eq!(
            reader.form_dialog_paths().next().unwrap().as_path(),
            native.form_source
        );
        assert!(
            reader
                .read_owner(reader.form_dialog_paths().next().unwrap())
                .unwrap()
                .source
                .form
                .is_some()
        );
        let accepted = AcceptedBase::open(
            &copied.root().join(".eska/accepted"),
            plan.accepted_reference(),
        )
        .unwrap();
        assert_eq!(
            accepted.permission(&frozen, Operation::Resource).reason,
            PermissionReason::EskaFrozen
        );
        let restored = test.0.join("restored accepted");
        accepted.restore(&restored).unwrap();
        assert_eq!(
            fs::read(restored.join(native.module_source)).unwrap(),
            fs::read(native.input.join(&native.module)).unwrap()
        );
        let output = test.0.join("native export");
        export_designer(copied.source(), &output).unwrap();
        assert!(output.join(&native.root).is_file());
        assert!(output.join(&native.form).is_file());
        for path in [&native.dialog, &native.schema] {
            assert_eq!(
                fs::read(output.join(path)).unwrap(),
                fs::read(native.input.join(path)).unwrap()
            );
        }
        assert_eq!(
            fs::read(output.join(&native.module)).unwrap(),
            b"\xef\xbb\xbf// local\r\n"
        );
        assert_eq!(fixture::snapshot(&native.input), original);
    }
}
