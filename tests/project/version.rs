use std::fs;

use eska::project::{
    ProjectType, discovery,
    version::{self, BumpKind, VersionError},
};

use crate::support::TestDir;

mod eska_source;
mod external;

const PREFIX: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\">\r\n\t<Configuration uuid=\"12345678-1234-1234-1234-123456789012\">\r\n\t\t<Properties>\r\n\t\t\t<Name>Пример</Name>\r\n\t\t\t<Version>";
const SUFFIX: &str = "</Version>\r\n\t\t\t<Comment>Не менять</Comment>\r\n\t\t</Properties>\r\n\t\t<ChildObjects><Catalog><Properties><Version>9.9.9.9</Version></Properties></Catalog></ChildObjects>\r\n\t</Configuration>\r\n</MetaDataObject>\r\n";

/// Create one discovered configuration project with a byte-sensitive descriptor.
fn project(version: &str) -> (TestDir, eska::project::Project) {
    let fixture = TestDir::new();
    let source = fixture.0.join("src");
    fs::create_dir(&source).expect("source");
    fs::write(
        fixture.0.join("eska.toml"),
        "[project]\ntype = 'configuration'\n",
    )
    .expect("config");
    fs::write(
        source.join("Configuration.xml"),
        format!("{PREFIX}{version}{SUFFIX}"),
    )
    .expect("descriptor");
    let discovered = discovery::discover(&fixture.0).expect("project");
    (fixture, discovered)
}

#[test]
fn bump_changes_only_the_root_version_value_bytes() {
    let (fixture, project) = project("1.0.2.01");
    let path = fixture.0.join("src/Configuration.xml");
    let before = fs::read(&path).expect("before");

    let outcome = version::bump(&project, BumpKind::Patch).expect("bump");

    assert_eq!(outcome.previous.to_string(), "1.0.2.01");
    assert_eq!(outcome.current.to_string(), "1.0.3.01");
    let expected = String::from_utf8(before)
        .expect("UTF-8")
        .replacen(
            "<Version>1.0.2.01</Version>",
            "<Version>1.0.3.01</Version>",
            1,
        )
        .into_bytes();
    let after = fs::read(path).expect("after");
    assert_eq!(after, expected);
    assert!(after.starts_with(&[0xef, 0xbb, 0xbf]));
    assert!(after.windows(2).any(|bytes| bytes == b"\r\n"));
    assert!(
        String::from_utf8(after)
            .unwrap()
            .contains("<Version>9.9.9.9</Version>")
    );
}

/// Three-part updates retain all surrounding bytes and never append a build value.
#[test]
fn three_part_bump_preserves_descriptor_bytes() {
    let (fixture, project) = project("1.2.3");
    let path = fixture.0.join("src/Configuration.xml");
    let outcome = version::bump(&project, BumpKind::Minor).unwrap();
    assert_eq!(outcome.current.to_string(), "1.3.0");
    assert_eq!(
        fs::read(&path).unwrap(),
        format!("{PREFIX}1.3.0{SUFFIX}").as_bytes()
    );
}

/// An explicit policy mismatch blocks both inspection and mutation before any write.
#[test]
fn explicit_scheme_is_checked_without_converting_source() {
    for (value, scheme) in [("1.2.3", "x.y.z.build"), ("1.2.3.4", "x.y.z")] {
        let (fixture, _) = project(value);
        fs::write(
            fixture.0.join("eska.toml"),
            format!("[project]\ntype = 'configuration'\n[version]\nscheme = '{scheme}'\n"),
        )
        .unwrap();
        let project = discovery::discover(&fixture.0).unwrap();
        let path = project.source().join("Configuration.xml");
        let before = fs::read(&path).unwrap();
        assert!(matches!(
            version::inspect(&project),
            Err(VersionError::SchemeMismatch { .. })
        ));
        assert!(matches!(
            version::bump(&project, BumpKind::Patch),
            Err(VersionError::SchemeMismatch { .. })
        ));
        assert_eq!(fs::read(path).unwrap(), before);
    }
}

#[test]
fn inspect_is_read_only_and_reports_the_descriptor() {
    let (fixture, project) = project("01.0.2.01");
    let path = fixture.0.join("src/Configuration.xml");
    let before = fs::read(&path).expect("before");
    let info = version::inspect(&project).expect("version");
    assert_eq!(info.path, path);
    assert_eq!(info.version.to_string(), "01.0.2.01");
    assert_eq!(fs::read(info.path).unwrap(), before);
}

#[test]
fn supports_named_external_descriptors() {
    for (kind, tag) in [
        (ProjectType::Processing, "ExternalDataProcessor"),
        (ProjectType::Report, "ExternalReport"),
    ] {
        let fixture = TestDir::new();
        let source = fixture.0.join("src");
        fs::create_dir(&source).expect("source");
        fs::write(
            fixture.0.join("eska.toml"),
            format!("[project]\ntype = '{}'\n", kind.as_str()),
        )
        .expect("config");
        fs::write(
            source.join("ИменованныйОбъект.xml"),
            format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><{tag}><Properties><Name>Пример</Name><Version>1.0.1.1</Version></Properties></{tag}></MetaDataObject>"),
        )
        .expect("descriptor");
        let ext = source.join("ИменованныйОбъект/Ext");
        fs::create_dir_all(&ext).unwrap();
        fs::write(
            ext.join("ObjectModule.bsl"),
            "Функция Версия() Экспорт Возврат \"1.0.1.1\"; КонецФункции",
        )
        .unwrap();
        let project = discovery::discover(&fixture.0).expect("project");
        assert_eq!(
            version::inspect(&project).unwrap().version.to_string(),
            "1.0.1.1"
        );
    }
}

#[test]
fn supports_configuration_extensions_in_the_standard_descriptor() {
    let fixture = TestDir::new();
    let source = fixture.0.join("src");
    fs::create_dir(&source).expect("source");
    fs::write(
        fixture.0.join("eska.toml"),
        "[project]\ntype = 'extension'\n",
    )
    .expect("config");
    fs::write(
        source.join("Configuration.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration><Properties><ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose><Version>1.0.1.1</Version></Properties></Configuration></MetaDataObject>",
    )
    .expect("descriptor");
    let project = discovery::discover(&fixture.0).expect("project");
    assert_eq!(
        version::inspect(&project).unwrap().version.to_string(),
        "1.0.1.1"
    );
}

#[test]
fn rejects_missing_ambiguous_and_invalid_root_versions() {
    for (value, expected) in [
        ("", "missing"),
        (
            "<Version>1.0.0.1</Version><Version>1.0.0.2</Version>",
            "ambiguous",
        ),
        ("<Version>one</Version>", "invalid"),
    ] {
        let (fixture, project) = project("1.0.0.1");
        let path = fixture.0.join("src/Configuration.xml");
        let input = format!(
            "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration><Properties>{value}</Properties></Configuration></MetaDataObject>"
        );
        fs::write(path, input).expect("replace descriptor");
        let error = version::inspect(&project).unwrap_err();
        assert!(
            matches!(
                (expected, error),
                ("missing", VersionError::VersionMissing { .. })
                    | ("ambiguous", VersionError::VersionAmbiguous { .. })
                    | ("invalid", VersionError::InvalidVersion { .. })
            ),
            "unexpected case {expected}"
        );
    }
}
