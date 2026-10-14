//! Complete offline project creation proves current form/query ownership for every native root.

mod ancestry;
use crate::query_creation_fixture as fixture;

use super::*;
use crate::project_core::source_format::{
    EXTERNAL_LAYOUT_VERSION, FormPropertyName, FormPropertyValue, NativeQueryDocument,
    QUERY_LAYOUT_VERSION, SourcePath,
};

/// All four profiles publish one editable source, an independently verifiable base and exact payloads.
#[test]
fn query_creation_preserves_four_native_profiles_and_complete_authoring() {
    for kind in fixture::TYPES {
        let test = TestDir::new();
        let native = fixture::native(&test.0, kind);
        let original = fixture::snapshot(&native.input);
        let destination = test.0.join("query project");
        let platform = PlatformVersion::parse("8.3.27.2325").unwrap();
        let frozen = NativeUuid::parse(fixture::FORM_UUID).unwrap();
        let imported = import_query_project(
            &native.input,
            &destination,
            name(),
            Some(&platform),
            std::slice::from_ref(&frozen),
        )
        .unwrap();
        assert_eq!(discover(&destination).unwrap(), imported.project);
        assert_eq!(
            imported.project.configuration().project_type().as_str(),
            kind
        );
        let source = EskaSource::open(imported.project, ProjectScope::Standalone).unwrap();
        let reader = source.reader();
        let external = matches!(kind, "processing" | "report");
        assert_eq!(
            reader.manifest().schema_version,
            if external {
                EXTERNAL_LAYOUT_VERSION
            } else {
                QUERY_LAYOUT_VERSION
            }
        );
        assert_eq!(reader.configuration().path.as_str(), native.root_source);
        assert_eq!(reader.configuration().declarations[0].name, "NativeName");
        assert_eq!(
            reader.configuration().declarations[0].uuid.as_str(),
            fixture::ROOT_UUID
        );
        assert_eq!(reader.query_paths().count(), 2);
        let form = SourcePath::parse(native.form_source.to_str().unwrap()).unwrap();
        assert_eq!(reader.form_dialog_paths().collect::<Vec<_>>(), [&form]);
        assert!(reader.read_form_source(&form, u64::MAX).is_ok());
        assert!(!source.project().source().join(&native.dialog).exists());
        for query in reader.query_paths() {
            assert_eq!(
                query.as_path().extension(),
                Some(std::ffi::OsStr::new("sdbl"))
            );
            assert!(!reader.read_query_text(query, u64::MAX).unwrap().is_empty());
        }
        assert_eq!(
            fs::read(source.project().source().join(native.module_source)).unwrap(),
            fs::read(native.input.join(&native.module)).unwrap()
        );
        let base =
            AcceptedBase::open(&destination.join(ACCEPTED_DIRECTORY), &imported.accepted).unwrap();
        assert_eq!(
            base.permission(&frozen, Operation::Resource).reason,
            PermissionReason::EskaFrozen
        );
        let prepared = source
            .prepare(
                &imported.accepted,
                &destination.join(DEFAULT_GENERATIONS_DIRECTORY),
                true,
            )
            .unwrap();
        assert!(prepared.diagnostics().is_empty());
        assert!(prepared.xml().join(&native.root).is_file());
        assert_eq!(
            fs::read(prepared.xml().join(&native.dialog)).unwrap(),
            fs::read(native.input.join(&native.dialog)).unwrap()
        );
        assert_eq!(
            fs::read(prepared.xml().join(&native.schema)).unwrap(),
            fs::read(native.input.join(&native.schema)).unwrap()
        );
        for file in prepared.files() {
            let actual = fs::read(prepared.xml().join(file.path.as_path())).unwrap();
            let expected = fs::read(native.input.join(file.path.as_path())).unwrap();
            if reader.manifest().files.iter().any(|entry| {
                entry.path == file.path && matches!(entry.role, FileRole::Descriptor { .. })
            }) {
                assert_eq!(
                    Descriptor::from_xml(std::str::from_utf8(&actual).unwrap()).unwrap(),
                    Descriptor::from_xml(std::str::from_utf8(&expected).unwrap()).unwrap()
                );
            } else {
                assert_eq!(actual, expected);
            }
        }
        assert_eq!(fixture::snapshot(&native.input), original);
        assert!(!destination.join(".git").exists());
        assert!(!destination.join("build").exists());
    }
}

/// Saved form/query edits retain their independent original base instead of becoming accepted input.
#[test]
fn query_created_projects_prepare_saved_edits_under_the_original_pin() {
    for kind in fixture::TYPES {
        let test = TestDir::new();
        let native = fixture::native(&test.0, kind);
        let imported =
            import_query_project(&native.input, &test.0.join("project"), name(), None, &[])
                .unwrap();
        let source = EskaSource::open(imported.project, ProjectScope::Standalone).unwrap();
        let before = fs::read(source.project().source().join("source.json")).unwrap();
        let base_root = source.project().root().join(ACCEPTED_DIRECTORY);
        let baseline = fixture::snapshot(&base_root);
        let form_path = SourcePath::parse(native.form_source.to_str().unwrap()).unwrap();
        let mut form = source.reader().read_owner(&form_path).unwrap().source;
        form.form
            .as_mut()
            .unwrap()
            .properties
            .insert(FormPropertyName::ReadOnly, FormPropertyValue::Bool(true));
        fs::write(
            source.project().source().join(&native.form_source),
            form.to_yaml().unwrap(),
        )
        .unwrap();
        let query = source
            .reader()
            .query_paths()
            .find(|path| {
                path.as_str().contains("/Templates/")
                    || path.as_str().starts_with("Templates/")
                    || path.as_str().starts_with("CommonTemplates/")
            })
            .unwrap();
        let edited = "SELECT 42;\r\n// saved query";
        fs::write(source.project().source().join(query.as_path()), edited).unwrap();
        let prepared = source
            .prepare(
                &imported.accepted,
                &source.project().root().join(DEFAULT_GENERATIONS_DIRECTORY),
                true,
            )
            .unwrap();
        assert!(prepared.diagnostics().is_empty());
        let schema = fs::read_to_string(prepared.xml().join(&native.schema)).unwrap();
        assert_eq!(
            NativeQueryDocument::from_xml(&schema).unwrap().queries()[0].text,
            edited
        );
        let dialog = fs::read_to_string(prepared.xml().join(&native.dialog)).unwrap();
        assert!(dialog.contains("<ReadOnly>true</ReadOnly>"));
        assert_eq!(fixture::snapshot(&base_root), baseline);
        assert_eq!(
            fs::read(source.project().source().join("source.json")).unwrap(),
            before
        );
        AcceptedBase::open(&base_root, &imported.accepted).unwrap();
    }
}

/// Incomplete composition and unsupported profiles remove only exclusively claimed output.
#[test]
fn query_creation_failures_preserve_input_existing_paths_and_default_contracts() {
    for kind in fixture::TYPES {
        for failure in [
            "native",
            "descriptor",
            "form",
            "query",
            "freeze",
            "existing",
        ] {
            let test = TestDir::new();
            let native = fixture::native(&test.0, kind);
            match failure {
                "native" => {
                    let file = native.input.join(&native.root);
                    fs::write(
                        &file,
                        fs::read_to_string(&file).unwrap().replace("2.20", "9.99"),
                    )
                    .unwrap();
                }
                "descriptor" => fs::remove_file(native.input.join(&native.form)).unwrap(),
                "form" => fs::write(native.input.join(&native.dialog), "<Form/>").unwrap(),
                "query" => fs::write(
                    native.input.join(&native.schema),
                    "<DataCompositionSchema/>",
                )
                .unwrap(),
                _ => {}
            }
            let input = fixture::snapshot(&native.input);
            let destination = test.0.join("refused");
            if failure == "existing" {
                fs::create_dir(&destination).unwrap();
                fs::write(destination.join("sentinel"), b"user bytes").unwrap();
            }
            let frozen = if failure == "freeze" {
                vec![NativeUuid::parse("90000000-0000-4000-8000-000000000001").unwrap()]
            } else {
                vec![]
            };
            let error = import_query_project(&native.input, &destination, name(), None, &frozen)
                .unwrap_err();
            assert_eq!(
                error.code,
                match failure {
                    "native" => "source_profile_unsupported",
                    "descriptor" => "source_descriptor_missing",
                    "form" => "source_form_profile_unverified",
                    "query" => "source_query_profile_unverified",
                    "freeze" => "source_freeze_object_unknown",
                    "existing" => "source_destination_exists",
                    _ => unreachable!(),
                }
            );
            if failure == "existing" {
                assert_eq!(
                    fs::read(destination.join("sentinel")).unwrap(),
                    b"user bytes"
                );
            } else {
                assert!(!destination.exists(), "{kind} {failure}: {error:?}");
            }
            assert_eq!(fixture::snapshot(&native.input), input);
        }
        if matches!(kind, "processing" | "report") {
            let test = TestDir::new();
            let native = fixture::native(&test.0, kind);
            for import in [
                import_project,
                import_separated_project,
                import_readable_project,
            ] {
                let destination = test.0.join("legacy refused");
                assert_eq!(
                    import(&native.input, &destination, name(), None, &[])
                        .unwrap_err()
                        .code,
                    "source_profile_unsupported"
                );
                assert!(!destination.exists());
            }
        }
    }
}
