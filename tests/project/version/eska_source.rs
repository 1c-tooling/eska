//! ESKA version edits retain one module authority and enforce independently accepted permissions.

use super::*;
use eska::project_core::{
    native_version::{ProjectVersion, bump_with_base, plan_migration_with_base},
    source_format::{
        AcceptedRef, NativeUuid, SourceReader, accept_source, import_designer_queries,
    },
};

const UUID: &str = "10000000-0000-4000-8000-000000000001";

/// Build an explicitly selected external profile; the imported BSL remains untouched.
fn external(
    kind: ProjectType,
    text: &str,
    frozen: bool,
) -> (TestDir, eska::project::Project, AcceptedRef) {
    let fixture = TestDir::new();
    let tag = if kind == ProjectType::Processing {
        "ExternalDataProcessor"
    } else {
        "ExternalReport"
    };
    fs::create_dir_all(fixture.0.join("native/Алиас.v1/Ext")).unwrap();
    fs::write(fixture.0.join("native/Алиас.v1.xml"), format!(
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{tag} uuid=\"{UUID}\"><Properties><Name>Пример</Name><Comment>Сохранить</Comment></Properties></{tag}></MetaDataObject>\r\n"
    )).unwrap();
    fs::write(fixture.0.join("native/Алиас.v1/Ext/ObjectModule.bsl"), text).unwrap();
    import_designer_queries(&fixture.0.join("native"), &fixture.0.join("src")).unwrap();
    fs::create_dir(fixture.0.join(".eska")).unwrap();
    let freeze = if frozen {
        vec![NativeUuid::parse(UUID).unwrap()]
    } else {
        vec![]
    };
    let pin = accept_source(
        &fixture.0.join("src"),
        &fixture.0.join(".eska/accepted"),
        &freeze,
    )
    .unwrap();
    fs::write(
        fixture.0.join("eska.toml"),
        format!(
            "[project]\ntype='{}'\nsource_format='eska'\n[eska]\naccepted_base='.eska/accepted'\n",
            kind.as_str()
        ),
    )
    .unwrap();
    let project = discovery::discover(&fixture.0).unwrap();
    (fixture, project, pin)
}

/// Three/four components, both external artifacts and exact BOM/CRLF survive protected bumps.
#[test]
fn versions_read_without_trust_and_bump_only_under_the_pinned_base() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        for (value, next) in [("1.2.3", "1.3.0"), ("01.02.003.0004", "01.03.001.0001")] {
            let text = format!(
                "\u{feff}// {value}\r\nФункция Версия() Экспорт\r\n\tВозврат \"{value}\";\r\nКонецФункции\r\n"
            );
            let (_fixture, project, pin) = external(kind, &text, false);
            let root = SourceReader::open(project.source())
                .unwrap()
                .configuration()
                .path
                .clone();
            let yaml = fs::read(project.source().join(root.as_path())).unwrap();
            let map = fs::read(project.source().join("source.json")).unwrap();
            assert_eq!(
                version::inspect(&project).unwrap().version.to_string(),
                value
            );
            assert_eq!(
                version::bump(&project, BumpKind::Minor).unwrap_err().code(),
                "accepted_base_pin_required"
            );
            let wrong = AcceptedRef {
                sha256: "0".repeat(64),
            };
            assert_eq!(
                bump_with_base(&project, BumpKind::Minor, Some(&wrong))
                    .unwrap_err()
                    .code(),
                "source_baseline_untrusted"
            );
            assert_eq!(
                fs::read_to_string(project.source().join("ObjectModule.bsl")).unwrap(),
                text
            );
            let outcome = bump_with_base(&project, BumpKind::Minor, Some(&pin)).unwrap();
            assert_eq!(outcome.current.to_string(), next);
            assert_eq!(
                fs::read_to_string(&outcome.path).unwrap(),
                text.replacen(
                    &format!("Возврат \"{value}\""),
                    &format!("Возврат \"{next}\""),
                    1
                )
            );
            assert_eq!(
                fs::read(project.source().join(root.as_path())).unwrap(),
                yaml
            );
            assert_eq!(fs::read(project.source().join("source.json")).unwrap(), map);
            fs::remove_dir_all(project.root().join(".eska/accepted")).unwrap();
            assert_eq!(
                version::inspect(&project).unwrap().version.to_string(),
                next
            );
        }
    }
}

/// Import preserves a missing function; explicit migration adopts it and is byte-idempotent.
#[test]
fn explicit_eska_migration_exports_and_binds_registration_without_duplicate_versions() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let text = "\u{feff}Функция СведенияОВнешнейОбработке() Экспорт\r\nРегистрация = Новый Структура;\r\nРегистрация.Вставить(\"Версия\", \"1.2.3\");\r\nВозврат Регистрация;\r\nКонецФункции\r\n";
        let (_fixture, project, pin) = external(kind, text, false);
        assert_eq!(
            version::inspect(&project).unwrap_err().code(),
            "version_function_missing"
        );
        let initial = ProjectVersion::parse("1.2.3").unwrap();
        let plan = plan_migration_with_base(&project, Some(&initial), Some(&pin)).unwrap();
        assert!(!plan.creates_module());
        assert_eq!(plan.edits().len(), 2);
        assert_eq!(fs::read_to_string(plan.path()).unwrap(), text);
        plan.publish().unwrap();
        let updated = fs::read_to_string(plan.path()).unwrap();
        assert_eq!(version::inspect(&project).unwrap().version, initial);
        assert!(updated.contains("\"Версия\", Версия()"));
        let repeated = plan_migration_with_base(&project, None, Some(&pin)).unwrap();
        assert!(repeated.edits().is_empty());
        repeated.publish().unwrap();
        assert_eq!(fs::read_to_string(plan.path()).unwrap(), updated);
    }
}

/// Frozen root ancestry blocks both migration and bump without treating the module as encrypted.
#[test]
fn frozen_external_root_cannot_grant_itself_version_permission() {
    for text in [
        "// Нет функции\r\n",
        "Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\r\n",
    ] {
        let (_fixture, project, pin) = external(ProjectType::Report, text, true);
        let initial = ProjectVersion::parse("1.2.3").unwrap();
        assert_eq!(
            plan_migration_with_base(&project, Some(&initial), Some(&pin))
                .unwrap_err()
                .code(),
            "source_edit_frozen"
        );
        if !text.starts_with("//") {
            assert_eq!(
                bump_with_base(&project, BumpKind::Patch, Some(&pin))
                    .unwrap_err()
                    .code(),
                "source_edit_frozen"
            );
        }
        assert_eq!(
            fs::read_to_string(project.source().join("ObjectModule.bsl")).unwrap(),
            text
        );
    }
}

/// Root, manifest, module and accepted-storage changes invalidate a prepared migration.
#[test]
fn stale_eska_migration_preserves_concurrent_source_and_policy_changes() {
    for target in [
        "report.yaml",
        "source.json",
        "ObjectModule.bsl",
        ".eska/accepted/accepted.json",
    ] {
        let (_fixture, project, pin) = external(ProjectType::Report, "// До миграции\r\n", false);
        let initial = ProjectVersion::parse("1.2.3").unwrap();
        let plan = plan_migration_with_base(&project, Some(&initial), Some(&pin)).unwrap();
        let path = if target.starts_with(".eska") {
            project.root().join(target)
        } else {
            project.source().join(target)
        };
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(b' ');
        fs::write(&path, &bytes).unwrap();
        assert!(plan.publish().is_err(), "{target}");
        assert_eq!(fs::read(&path).unwrap(), bytes);
        if target != "ObjectModule.bsl" {
            assert_eq!(
                fs::read_to_string(plan.path()).unwrap(),
                "// До миграции\r\n"
            );
        }
    }
}
