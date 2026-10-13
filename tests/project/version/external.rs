use super::*;
use eska::project::version::{ProjectVersion, VersionScheme};
use eska::project_core::{object_module_version::ContractErrorKind, publish::PublishError};

/// A native external object never needs a synthetic XML version property.
fn external(kind: ProjectType) -> (TestDir, eska::project::Project) {
    let fixture = TestDir::new();
    fs::create_dir(fixture.0.join("src")).unwrap();
    fs::write(
        fixture.0.join("eska.toml"),
        format!("[project]\ntype = '{}'\n", kind.as_str()),
    )
    .unwrap();
    let tag = if kind == ProjectType::Processing {
        "ExternalDataProcessor"
    } else {
        "ExternalReport"
    };
    fs::write(fixture.0.join("src/ИменованныйОбъект.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><{tag}><Properties><Name>ИменованныйОбъект</Name></Properties></{tag}></MetaDataObject>\r\n")).unwrap();
    let project = discovery::discover(&fixture.0).unwrap();
    (fixture, project)
}

/// Helpers create only the fixture's selected native module layout.
fn module(project: &eska::project::Project, direct: bool, text: &str) -> std::path::PathBuf {
    let ext = project.source().join(if direct {
        "Ext"
    } else {
        "ИменованныйОбъект/Ext"
    });
    fs::create_dir_all(&ext).unwrap();
    let path = ext.join("ObjectModule.bsl");
    fs::write(&path, text).unwrap();
    path
}

/// A bump changes only one BSL literal across both artifacts, layouts and numeric schemes.
#[test]
fn external_bump_preserves_module_and_metadata_bytes() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        for direct in [false, true] {
            for (value, next, scheme) in [
                ("1.2.3", "1.3.0", VersionScheme::ThreePart),
                ("01.02.003.0004", "01.03.001.0001", VersionScheme::FourPart),
            ] {
                let (_fixture, project) = external(kind);
                let before = format!(
                    "\u{feff}// Сохранить {value}\r\nФункция Версия() Экспорт\r\n\tВозврат \"{value}\"; // Номер\r\nКонецФункции\r\nФункция СведенияОВнешнейОбработке() Экспорт\r\n\tРегистрация = Новый Структура;\r\n\tРегистрация.Вставить(\"Версия\", Версия());\r\n\tРегистрация.Информация = \"{value}\";\r\n\tВозврат Регистрация;\r\nКонецФункции\r\n"
                );
                let path = module(&project, direct, &before);
                let descriptor = project.source().join("ИменованныйОбъект.xml");
                let xml = fs::read(&descriptor).unwrap();
                let info = version::inspect(&project).unwrap();
                assert_eq!(info.path, path);
                assert_eq!(info.version.scheme(), scheme);
                assert_eq!(fs::read_to_string(&path).unwrap(), before);
                let permissions = fs::metadata(&path).unwrap().permissions();
                let outcome = version::bump(&project, BumpKind::Minor).unwrap();
                assert_eq!(outcome.current.to_string(), next);
                assert_eq!(
                    fs::read_to_string(&path).unwrap(),
                    before.replacen(
                        &format!("Возврат \"{value}\""),
                        &format!("Возврат \"{next}\""),
                        1
                    )
                );
                assert_eq!(fs::metadata(&path).unwrap().permissions(), permissions);
                assert_eq!(fs::read(descriptor).unwrap(), xml);
            }
        }
    }
}

/// Missing functions are adopted only by an explicit plan, including missing-module creation.
#[test]
fn migration_creates_missing_module_and_exports_existing_function() {
    for existing in [false, true] {
        let (_fixture, project) = external(ProjectType::Report);
        if existing {
            module(
                &project,
                false,
                "\u{feff}Функция Версия() // signature\r\nВозврат \"1.2.3\"; КонецФункции\r\n",
            );
        }
        let initial = ProjectVersion::parse("1.2.3").unwrap();
        let plan = version::plan_migration(&project, (!existing).then_some(&initial)).unwrap();
        assert_eq!(plan.creates_module(), !existing);
        assert_eq!(plan.edits().len(), 1);
        if !existing {
            assert!(!project.source().join("ИменованныйОбъект").exists());
        }
        plan.publish().unwrap();
        assert_eq!(version::inspect(&project).unwrap().version, initial);
        assert!(
            version::plan_migration(&project, None)
                .unwrap()
                .edits()
                .is_empty()
        );
        if existing {
            assert!(
                fs::read_to_string(plan.path())
                    .unwrap()
                    .contains("() Экспорт // signature\r\n")
            );
        }
    }
}

/// The descriptor or module cannot be changed between preparing and publishing a migration.
#[test]
fn stale_migration_preserves_concurrent_source_changes() {
    for change_descriptor in [false, true] {
        let (_fixture, project) = external(ProjectType::Processing);
        let initial = ProjectVersion::parse("1.2.3").unwrap();
        let plan = version::plan_migration(&project, Some(&initial)).unwrap();
        if change_descriptor {
            fs::write(project.source().join("ИменованныйОбъект.xml"), "<changed/>").unwrap();
        } else {
            module(&project, false, "// Concurrent module");
        }
        assert!(matches!(
            plan.publish(),
            Err(VersionError::Publication {
                error: PublishError::Conflict,
                ..
            })
        ));
        if change_descriptor {
            assert!(!plan.path().exists());
        } else {
            assert_eq!(
                fs::read_to_string(plan.path()).unwrap(),
                "// Concurrent module"
            );
        }
    }
}

/// Protected binaries, duplicate locations and an unsuitable body have no fallback source.
#[test]
fn refuses_protected_ambiguous_and_unmanaged_modules() {
    let (_fixture, project) = external(ProjectType::Processing);
    assert!(matches!(
        version::inspect(&project),
        Err(VersionError::ModuleMissing { .. })
    ));
    let path = module(
        &project,
        false,
        "Функция Версия() Экспорт Возврат Номер; КонецФункции",
    );
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        version::bump(&project, BumpKind::Patch),
        Err(VersionError::ModuleContract {
            error: eska::project_core::object_module_version::ContractError {
                kind: ContractErrorKind::InvalidBody,
                ..
            },
            ..
        })
    ));
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(path.with_extension("bin"), b"protected").unwrap();
    assert!(matches!(
        version::inspect(&project),
        Err(VersionError::ModuleProtected { .. })
    ));
    assert!(matches!(
        version::plan_migration(&project, None),
        Err(VersionError::ModuleProtected { .. })
    ));
    fs::remove_file(path.with_extension("bin")).unwrap();
    module(
        &project,
        true,
        "Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции",
    );
    assert!(matches!(
        version::inspect(&project),
        Err(VersionError::ModuleAmbiguous { .. })
    ));
}

/// Readable modules remain inspectable, but read-only permissions cannot be bypassed by bump/migrate.
#[cfg(unix)]
#[test]
fn read_only_and_symlink_sources_refuse_writes() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (fixture, project) = external(ProjectType::Processing);
    let text = "Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции";
    let path = module(&project, false, text);
    let permissions = fs::metadata(&path).unwrap().permissions();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    assert!(version::inspect(&project).is_ok());
    assert!(matches!(
        version::bump(&project, BumpKind::Patch),
        Err(VersionError::Publication {
            error: PublishError::ReadOnly,
            ..
        })
    ));
    fs::set_permissions(&path, permissions).unwrap();
    fs::write(&path, text.replace(" Экспорт", "")).unwrap();
    let plan = version::plan_migration(&project, None).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    assert!(matches!(
        plan.publish(),
        Err(VersionError::Publication {
            error: PublishError::ReadOnly,
            ..
        })
    ));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    fs::remove_file(&path).unwrap();
    let outside = fixture.0.join("other.bsl");
    fs::write(&outside, text).unwrap();
    symlink(&outside, &path).unwrap();
    assert!(matches!(
        version::inspect(&project),
        Err(VersionError::Publication {
            error: PublishError::UnsafePath,
            ..
        })
    ));
    assert!(matches!(
        version::plan_migration(&project, None),
        Err(VersionError::Publication {
            error: PublishError::UnsafePath,
            ..
        })
    ));
    assert_eq!(fs::read_to_string(outside).unwrap(), text);
}
