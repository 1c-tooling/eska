use super::*;

mod queries;
use crate::{
    project_core::{
        discovery::discover,
        eska_source::EskaSource,
        metadata_model::ProjectScope,
        source_format::{
            AcceptedBase, Descriptor, FileRole, Operation, PermissionReason, PropertyName,
        },
    },
    test_support::TestDir,
};

/// Keep native input small and read-only while exercising actual reversible bytes and directories.
fn native(root: &Path) -> PathBuf {
    let input = root.join("native");
    fs::create_dir(&input).expect("native directory");
    fs::write(input.join("Configuration.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>NativeName</Name><Comment>Accepted</Comment><Version>1.2.3</Version></Properties></Configuration></MetaDataObject>"
    ).expect("configuration");
    fs::write(input.join("Module.bsl"), b"\xef\xbb\xbf// native\r\n").expect("BSL");
    fs::create_dir(input.join("Empty")).expect("empty native directory");
    input
}

/// Parse a checked project label without confusing it with the native metadata name.
fn name() -> ProjectName {
    ProjectName::parse("project-label".into()).expect("project label")
}

/// Copy the incomplete supplier fixture into an own native test input without support rules.
fn readable_native(root: &Path) -> PathBuf {
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let input = root.join("native");
    copy_tree(&original, &input);
    fs::remove_file(input.join("Ext/ParentConfigurations.bin")).unwrap();
    input
}

/// Tests mutate only their own native fixture, preserving the repository's input bytes.
fn copy_tree(input: &Path, output: &Path) {
    fs::create_dir(output).unwrap();
    for entry in fs::read_dir(input).unwrap() {
        let entry = entry.unwrap();
        let destination = output.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

#[test]
/// One operation creates discoverable inputs, transport and a verifiable candidate pin.
fn complete_project_preserves_native_name_version_and_payloads() {
    let test = TestDir::new();
    let input = native(&test.0);
    let destination = test.0.join("new project");
    let platform = PlatformVersion::parse("8.3.27.2325").expect("platform");
    let imported = import_project(&input, &destination, name(), Some(&platform), &[])
        .expect("complete project");
    assert_eq!(discover(&destination).expect("discovery"), imported.project);
    let config = ProjectConfig::load(&destination.join("eska.toml")).expect("manifest");
    assert_eq!(
        config.name().expect("project name").as_str(),
        "project-label"
    );
    assert_eq!(
        config.configuration().build_settings().platform_version(),
        Some(&platform)
    );
    AcceptedBase::open(&destination.join(ACCEPTED_DIRECTORY), &imported.accepted).expect("pin");
    let source = EskaSource::open(imported.project, ProjectScope::Standalone).expect("source");
    assert_eq!(
        source.reader().configuration().declarations[0].name,
        "NativeName"
    );
    let output = source
        .prepare(
            &imported.accepted,
            &destination.join(DEFAULT_GENERATIONS_DIRECTORY),
            true,
        )
        .expect("protected output");
    assert_eq!(
        fs::read(output.xml().join("Module.bsl")).expect("output BSL"),
        fs::read(input.join("Module.bsl")).expect("input BSL")
    );
    assert!(output.xml().join("Empty").is_dir());
    assert!(
        fs::read_to_string(destination.join(".gitignore"))
            .expect("ignore")
            .contains("/.eska/generations/")
    );
    assert!(
        fs::read_to_string(destination.join(".gitattributes"))
            .expect("attributes")
            .contains("\"/.eska/accepted/**\" -text -filter")
    );
}

#[test]
/// An offline import does not require a platform; existing directories and overlaps stay intact.
fn offline_import_and_exclusive_destination_rules() {
    let test = TestDir::new();
    let input = native(&test.0);
    let existing = test.0.join("existing");
    fs::create_dir(&existing).expect("existing user directory");
    fs::write(existing.join("sentinel"), b"user bytes").expect("sentinel");
    assert_eq!(
        import_project(&input, &existing, name(), None, &[])
            .expect_err("existing destination")
            .code,
        "source_destination_exists"
    );
    assert_eq!(
        fs::read(existing.join("sentinel")).expect("preserved"),
        b"user bytes"
    );
    assert_eq!(
        import_project(&input, &input.join("nested"), name(), None, &[])
            .expect_err("overlap")
            .code,
        "source_tree_overlap"
    );
    assert!(!input.join("nested").exists());
    let created = import_project(&input, &test.0.join("offline"), name(), None, &[])
        .expect("offline creation");
    assert_eq!(
        created
            .project
            .configuration()
            .build_settings()
            .platform_version(),
        None
    );
}

#[test]
/// A malformed profile or unknown freeze target removes only the newly claimed output directory.
fn failed_import_cleans_its_own_incomplete_project() {
    let test = TestDir::new();
    let input = native(&test.0);
    let before = fs::read(input.join("Configuration.xml")).expect("original input");
    let unknown = NativeUuid::parse("20000000-0000-4000-8000-000000000001").expect("UUID");
    let destination = test.0.join("invalid-freeze");
    assert_eq!(
        import_project(&input, &destination, name(), None, &[unknown])
            .expect_err("freeze")
            .code,
        "source_freeze_object_unknown"
    );
    assert!(!destination.exists());
    assert_eq!(
        fs::read(input.join("Configuration.xml")).expect("unchanged input"),
        before
    );
    fs::write(
        input.join("Configuration.xml"),
        String::from_utf8(before)
            .expect("UTF-8")
            .replace("version=\"2.20\"", "version=\"9.99\""),
    )
    .expect("unsupported profile");
    let destination = test.0.join("unsupported");
    assert_eq!(
        import_project(&input, &destination, name(), None, &[])
            .expect_err("profile")
            .code,
        "source_profile_unsupported"
    );
    assert!(!destination.exists());
}

/// The opt-in creation path publishes readable source, its same-layout base and byte-safe transport.
#[test]
fn readable_project_preserves_native_transport_and_frozen_policy() {
    let test = TestDir::new();
    let input = readable_native(&test.0);
    let destination = test.0.join("readable");
    let frozen = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
    let imported = import_readable_project(
        &input,
        &destination,
        name(),
        None,
        std::slice::from_ref(&frozen),
    )
    .unwrap();
    assert_eq!(discover(&destination).unwrap(), imported.project);
    let base =
        AcceptedBase::open(&destination.join(ACCEPTED_DIRECTORY), &imported.accepted).unwrap();
    assert_eq!(
        base.permission(&frozen, Operation::Property(PropertyName::Comment))
            .reason,
        PermissionReason::EskaFrozen
    );
    let source = EskaSource::open(imported.project, ProjectScope::Standalone).unwrap();
    assert_eq!(source.reader().manifest().schema_version, 3);
    for owner in source.reader().owner_paths() {
        let snapshot = source.reader().read_owner(owner).unwrap();
        assert_eq!(snapshot.source.document.descriptor.schema_version, 2);
        assert!(snapshot.source.nested.is_empty());
    }
    assert_eq!(
        fs::read(destination.join("src/CommonModules/Tools/Module.bsl")).unwrap(),
        fs::read(input.join("CommonModules/Tools/Ext/Module.bsl")).unwrap()
    );
    assert!(
        destination
            .join("src/Catalogs/Sample/Forms/Main/form.yaml")
            .is_file()
    );
    assert!(!destination.join("src/Ext").exists());
    let prepared = source
        .prepare(
            &imported.accepted,
            &destination.join(DEFAULT_GENERATIONS_DIRECTORY),
            true,
        )
        .unwrap();
    assert!(prepared.diagnostics().is_empty());
    for file in prepared.files() {
        let actual = fs::read(prepared.xml().join(file.path.as_path())).unwrap();
        let original = fs::read(input.join(file.path.as_path())).unwrap();
        let descriptor = source.reader().manifest().files.iter().any(|entry| {
            entry.path == file.path && matches!(entry.role, FileRole::Descriptor { .. })
        });
        if descriptor {
            // The synthetic fixture mixes inline/minified whitespace; compare native model facts.
            assert_eq!(
                Descriptor::from_xml(std::str::from_utf8(&actual).unwrap()).unwrap(),
                Descriptor::from_xml(std::str::from_utf8(&original).unwrap()).unwrap()
            );
        } else {
            assert_eq!(actual, original);
        }
    }
    assert!(!destination.join(".git").exists());
    assert!(!destination.join("build").exists());
}

/// A readable import failure still removes only the project exclusively claimed by this operation.
#[test]
fn readable_import_failure_preserves_input_and_existing_destinations() {
    let test = TestDir::new();
    let input = native(&test.0);
    let original = fs::read(input.join("Configuration.xml")).unwrap();
    let unknown = NativeUuid::parse("90000000-0000-4000-8000-000000000001").unwrap();
    let failed = test.0.join("failed");
    assert_eq!(
        import_readable_project(&input, &failed, name(), None, &[unknown])
            .unwrap_err()
            .code,
        "source_freeze_object_unknown"
    );
    assert!(!failed.exists());
    let existing = test.0.join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("sentinel"), b"user bytes").unwrap();
    assert_eq!(
        import_readable_project(&input, &existing, name(), None, &[])
            .unwrap_err()
            .code,
        "source_destination_exists"
    );
    assert_eq!(fs::read(existing.join("sentinel")).unwrap(), b"user bytes");
    assert_eq!(fs::read(input.join("Configuration.xml")).unwrap(), original);
}
