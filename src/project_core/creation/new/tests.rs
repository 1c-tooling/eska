//! Fresh project templates retain one authoring source and independent original authorities.

use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt::Write, fs, path::Path};

use super::*;
use crate::{
    project_core::{
        Project,
        creation::import_query_project,
        discovery::discover,
        eska_source::EskaSource,
        metadata_model::ProjectScope,
        native_version,
        source_format::{
            AcceptedBase, PropertyName, PropertyValue, SourceReader, project_import_ancestor,
        },
    },
    test_support::TestDir,
};

const TYPES: [ProjectType; 4] = [
    ProjectType::Configuration,
    ProjectType::Extension,
    ProjectType::Processing,
    ProjectType::Report,
];

/// Reuse the existing numeric parser, including its preserved four-part width contract.
fn options(kind: ProjectType, version: &str) -> NewProjectOptions {
    NewProjectOptions::new(kind, ProjectVersion::parse(version).unwrap())
}

/// Independently fingerprint captured settings bytes without optional digest formatting features.
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut value, byte| {
            write!(value, "{byte:02x}").unwrap();
            value
        })
}

/// CF/CFE keep their value in YAML; the existing native-version facade supports ESKA external modules.
fn current_version(project: &Project) -> String {
    if matches!(
        project.configuration().project_type(),
        ProjectType::Processing | ProjectType::Report
    ) {
        return native_version::inspect(project)
            .unwrap()
            .version
            .to_string();
    }
    let reader = SourceReader::open(project.source()).unwrap();
    let owner = reader.read_owner(&reader.configuration().path).unwrap();
    let Some(PropertyValue::Text(version)) = owner
        .source
        .document
        .descriptor
        .object
        .property(PropertyName::Version)
    else {
        panic!("expected the sole authoritative YAML version");
    };
    version.clone()
}

/// Physical descriptors expose fresh native identities independently of any package hash.
fn identities(xml: &str) -> BTreeSet<String> {
    let document = roxmltree::Document::parse(xml).unwrap();
    document
        .descendants()
        .filter_map(|node| {
            node.attribute("uuid")
                .or_else(|| {
                    matches!(node.tag_name().name(), "ObjectId" | "TypeId" | "ValueId")
                        .then(|| node.text())
                        .flatten()
                })
                .map(str::to_owned)
        })
        .collect()
}

/// Temporary native siblings disappear after publication and are never mistaken for editable sources.
fn no_staging(parent: &Path) {
    assert!(fs::read_dir(parent).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".eska-new-")
    }));
}

/// Every type begins with a minimal composition and keeps all persisted support/ancestry independent.
#[test]
fn four_new_profiles_have_one_source_fresh_ids_and_independent_initial_authorities() {
    let test = TestDir::new();
    let mut seen = BTreeSet::new();
    for kind in TYPES {
        for index in 0..2 {
            let destination = test.0.join(format!("{}-{index}", kind.as_str()));
            let label = ProjectName::parse("2-project-label".into()).unwrap();
            let options = options(kind, INITIAL_PROJECT_VERSION);
            let imported = new_project(&destination, label, &options).unwrap();
            assert_eq!(discover(&destination).unwrap(), imported.project);
            assert_eq!(imported.project.configuration().project_type(), kind);
            assert!(
                imported
                    .project
                    .configuration()
                    .build_settings()
                    .platform_version()
                    .is_none()
            );
            let source = EskaSource::open(imported.project, ProjectScope::Standalone).unwrap();
            let reader = source.reader();
            assert_eq!(reader.owner_paths().count(), 1);
            assert_eq!(reader.query_paths().count(), 0);
            assert_eq!(reader.form_dialog_paths().count(), 0);
            assert_eq!(
                reader.configuration().declarations[0].name,
                "Project_2_project_label"
            );
            AcceptedBase::open(&destination.join(".eska/accepted"), &imported.accepted).unwrap();
            let ancestor = imported.initial_ancestor.as_ref().unwrap();
            assert_ne!(ancestor.sha256(), imported.accepted.sha256);
            project_import_ancestor(source.project(), &ProjectScope::Standalone, ancestor).unwrap();
            assert_eq!(current_version(source.project()), INITIAL_PROJECT_VERSION);
            let prepared = source
                .prepare(
                    &imported.accepted,
                    &destination.join(".eska/generations"),
                    true,
                )
                .unwrap();
            assert!(prepared.diagnostics().is_empty());
            let root = prepared.xml().join(
                if matches!(kind, ProjectType::Configuration | ProjectType::Extension) {
                    "Configuration.xml"
                } else {
                    "Project_2_project_label.xml"
                },
            );
            let xml = fs::read_to_string(root).unwrap();
            let ids = identities(&xml);
            assert_eq!(
                ids.len(),
                if matches!(kind, ProjectType::Configuration | ProjectType::Extension) {
                    8
                } else {
                    4
                }
            );
            for id in ids {
                assert_eq!(id.as_bytes()[14], b'4');
                assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
                assert!(seen.insert(id));
            }
            if kind == ProjectType::Extension {
                assert!(xml.contains("<ObjectBelonging>Adopted</ObjectBelonging>"));
                assert!(xml.contains(
                    "<ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>"
                ));
            }
            assert_eq!(
                reader.module_paths().count(),
                usize::from(matches!(
                    kind,
                    ProjectType::Processing | ProjectType::Report
                ))
            );
            assert!(!destination.join(".git").exists());
            assert!(!destination.join("build").exists());
            no_staging(&test.0);
        }
    }
}

/// An explicit name stays exact; four-part version widths reach XML and the sole external function.
#[test]
fn explicit_native_names_and_four_part_versions_are_not_normalized() {
    for kind in TYPES {
        let test = TestDir::new();
        let opts = options(kind, "01.02.003.0004")
            .with_metadata_name("Новая_Конфигурация".into())
            .unwrap()
            .with_platform(Some(PlatformVersion::parse("8.3.27.2325").unwrap()));
        let imported = new_project(
            &test.0.join("project"),
            ProjectName::parse("label".into()).unwrap(),
            &opts,
        )
        .unwrap();
        let reader = SourceReader::open(imported.project.source()).unwrap();
        assert_eq!(
            reader.configuration().declarations[0].name,
            "Новая_Конфигурация"
        );
        assert_eq!(current_version(&imported.project), "01.02.003.0004");
        assert_eq!(
            imported.project.configuration().version_scheme(),
            Some(native_version::VersionScheme::FourPart)
        );
        assert_eq!(
            imported
                .project
                .configuration()
                .build_settings()
                .platform_version()
                .unwrap()
                .as_str(),
            "8.3.27.2325"
        );
        if matches!(kind, ProjectType::Processing | ProjectType::Report) {
            let module = fs::read(imported.project.source().join("ObjectModule.bsl")).unwrap();
            assert!(module.starts_with(&[0xef, 0xbb, 0xbf]));
            let body = std::str::from_utf8(&module).unwrap();
            assert!(body.contains("#Область ПрограммныйИнтерфейс\r\n"));
            assert!(body.contains("// Возвращаемое значение:\r\n//  Строка"));
            assert!(body.contains("Возврат \"01.02.003.0004\";"));
            assert!(body.ends_with("#КонецОбласти\r\n"));
            assert!(
                reader
                    .read_owner(&reader.configuration().path)
                    .unwrap()
                    .source
                    .document
                    .descriptor
                    .object
                    .property(PropertyName::Version)
                    .is_none()
            );
        }
        no_staging(&test.0);
    }
}

/// Validation precedes ownership; occupied foreign directories and unsafe paths are left intact.
#[test]
fn invalid_names_and_occupied_destinations_do_not_claim_or_delete_files() {
    let test = TestDir::new();
    for invalid in [
        "", "2Bad", "Bad name", "../Bad", "A&B", "Bad-Name", "CON", "NUL",
    ] {
        assert_eq!(
            options(ProjectType::Configuration, INITIAL_PROJECT_VERSION)
                .with_metadata_name(invalid.into())
                .unwrap_err()
                .code,
            "source_metadata_name_invalid"
        );
        no_staging(&test.0);
    }
    let occupied = test.0.join("existing");
    fs::create_dir(&occupied).unwrap();
    fs::write(occupied.join("sentinel"), "user bytes").unwrap();
    assert_eq!(
        new_project(
            &occupied,
            ProjectName::parse("label".into()).unwrap(),
            &options(ProjectType::Configuration, INITIAL_PROJECT_VERSION)
        )
        .unwrap_err()
        .code,
        "source_destination_exists"
    );
    assert_eq!(fs::read(occupied.join("sentinel")).unwrap(), b"user bytes");
    assert_eq!(
        new_project(
            &test.0.join("CON"),
            ProjectName::parse("label".into()).unwrap(),
            &options(ProjectType::Configuration, INITIAL_PROJECT_VERSION)
        )
        .unwrap_err()
        .code,
        "source_path_invalid"
    );
    assert!(!test.0.join("CON").exists());
    no_staging(&test.0);
}

/// A failed real import cleans its destination and the separate exclusively owned native stage.
#[test]
fn failed_staged_import_removes_only_operation_owned_directories() {
    let test = TestDir::new();
    let destination = test.0.join("failed");
    let stage = template::NativeStage::claim(&destination).unwrap();
    let stage_path = stage.path.clone();
    fs::write(stage.path.join("Configuration.xml"), "invalid XML").unwrap();
    let result = import_query_project(
        &stage.path,
        &destination,
        ProjectName::parse("label".into()).unwrap(),
        None,
        &[],
    );
    assert!(stage.finish(result).is_err());
    assert!(!stage_path.exists());
    assert!(!destination.exists());
    no_staging(&test.0);
}

/// Cleanup failures cannot be silently promoted to successful new-project publication.
#[test]
fn missing_owned_stage_is_an_explicit_cleanup_failure() {
    let test = TestDir::new();
    let stage = template::NativeStage::claim(&test.0.join("new")).unwrap();
    fs::remove_dir(&stage.path).unwrap();
    assert_eq!(
        stage
            .finish(Err(Error::root("source_schema_invalid")))
            .unwrap_err()
            .code,
        "source_cleanup_failed"
    );
}

/// The returned manifest authority describes published bytes, never a later filesystem reread.
#[test]
fn published_manifest_digest_survives_later_comment_rewrites() {
    let test = TestDir::new();
    let created = new_project(
        &test.0.join("project"),
        ProjectName::parse("label".into()).unwrap(),
        &options(ProjectType::Configuration, INITIAL_PROJECT_VERSION),
    )
    .unwrap();
    let file = created.project.root().join("eska.toml");
    let original = fs::read(&file).unwrap();
    let original_digest = digest(&original);
    assert_eq!(created.manifest_sha256, original_digest);
    let mut rewritten = original;
    rewritten.extend_from_slice(b"# A later caller-owned edit.\r\n");
    fs::write(file, &rewritten).unwrap();
    assert_ne!(digest(&rewritten), created.manifest_sha256);
    assert_eq!(created.manifest_sha256, original_digest);
}
