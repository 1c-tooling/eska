use super::*;
use crate::{
    project_core::source_format::{
        FileOrigin, InputAuthority, InputFile, PROFILE, RULES_VERSION, SCHEMA_VERSION,
        SOURCE_MAP_VERSION,
    },
    test_support::TestDir,
};

/// Ordered address-only DTOs exercise lookup; these fixtures are not protected export proofs.
fn map(paths: &[&str]) -> SourceMap {
    let mut files: Vec<_> = paths
        .iter()
        .map(|path| {
            let path = SourcePath::parse(path).expect("native relative path");
            let input = InputFile {
                path: path.clone(),
                bytes: 0,
                sha256: "0".repeat(64),
            };
            MappedFile {
                path,
                bytes: 0,
                sha256: input.sha256.clone(),
                origin: FileOrigin::Payload {
                    authority: InputAuthority::Working,
                    input,
                    module: false,
                    owner: None,
                },
            }
        })
        .collect();
    files.sort_by(|left, right| left.path.cmp(&right.path));
    SourceMap {
        schema_version: SOURCE_MAP_VERSION,
        source_schema_version: SCHEMA_VERSION,
        rules_version: RULES_VERSION,
        profile: PROFILE.to_owned(),
        generation: "0".repeat(64),
        input_revision: "0".repeat(64),
        baseline: "0".repeat(64),
        native_revision: "0".repeat(64),
        directories: vec![],
        files,
    }
}

#[test]
/// Recorded ibcmd file-address shape retains spaces, Unicode and either stream's level.
fn recognizes_snapshot_addresses_and_portable_relative_separators() {
    let fixture = TestDir::new();
    let root = fixture.0.join("native, snapshot");
    let map = map(&["Catalogs/Объект с пробелами/Ext/Help/ru.html"]);
    for level in ["WARN", "ERROR", "FATAL"] {
        for path in [
            "Catalogs/Объект с пробелами/Ext/Help/ru.html",
            "Catalogs\\Объект с пробелами\\Ext\\Help\\ru.html",
        ] {
            let line = format!(
                "[{level}] File: {}/{path}, diagnostic text.\r\n",
                root.display()
            );
            assert_eq!(
                locate_output_file(&map, &root, line.as_bytes())
                    .expect("known native file")
                    .path,
                map.files[0].path
            );
        }
    }
}

#[test]
/// Foreign roots, missing files, aliases and unsupported messages never select another source.
fn unsupported_addresses_and_encodings_remain_unmapped() {
    let fixture = TestDir::new();
    let root = fixture.0.join("snapshot");
    let map = map(&["Configuration.xml"]);
    let outside = fixture.0.join("other/Configuration.xml");
    let lines = [
        format!("[WARN] File: {}, message", outside.display()),
        format!(
            "[WARN] File: {}-old/Configuration.xml, message",
            root.display()
        ),
        format!("[WARN] File: {}/Missing.xml, message", root.display()),
        format!(
            "[WARN] File: {}/../Configuration.xml, message",
            root.display()
        ),
        format!(
            "[WARN] File: {}/./Configuration.xml, message",
            root.display()
        ),
        format!("[WARN] File: {}/Configuration.xml", root.display()),
        format!("[INFO] File: {}/Configuration.xml, message", root.display()),
        format!(
            "[WARN] Файл: {}/Configuration.xml, сообщение",
            root.display()
        ),
    ];
    for line in lines {
        assert!(
            locate_output_file(&map, &root, line.as_bytes()).is_none(),
            "{line}"
        );
    }
    assert!(locate_output_file(&map, &root, b"[WARN] File: \xff, invalid encoding").is_none());
    assert!(
        locate_output_file(
            &map,
            Path::new("relative"),
            b"[WARN] File: relative/Configuration.xml, message"
        )
        .is_none()
    );
}

#[test]
/// Commas remain valid filename characters, but two possible known addresses cannot be guessed.
fn comma_filenames_are_matched_only_when_unambiguous() {
    let fixture = TestDir::new();
    let root = fixture.0.join("snapshot");
    let line = format!("[WARN] File: {}/a, b.xml, diagnostic text.", root.display());
    let unique = map(&["a, b.xml"]);
    assert_eq!(
        locate_output_file(&unique, &root, line.as_bytes())
            .expect("comma filename")
            .path
            .as_str(),
        "a, b.xml"
    );
    let ambiguous = map(&["a", "a, b.xml"]);
    assert!(locate_output_file(&ambiguous, &root, line.as_bytes()).is_none());
    let punctuated = map(&["a,b.xml"]);
    let line = format!(
        "[WARN] File: {}/a,b.xml, diagnostic, additional text.",
        root.display()
    );
    assert_eq!(
        locate_output_file(&punctuated, &root, line.as_bytes())
            .expect("comma without delimiter space")
            .path
            .as_str(),
        "a,b.xml"
    );
}

#[test]
/// The installed platform's direct and wrapped XDTO failures identify a resource without columns.
fn observed_xdto_errors_map_the_named_resource_and_reject_unrecognized_wrappers() {
    let fixture = TestDir::new();
    let root = fixture.0.join("snapshot");
    let path = "Catalogs/Sample/Forms/Main/Ext/Form.xml";
    let map = map(&[path]);
    let diagnostic = format!(
        "XDTO exception while reading file {}/{path}. Error converting XDTO data: Value: invalid\tType: {{http://www.w3.org/2001/XMLSchema}}decimal Error transforming XML data",
        root.display()
    );
    for prefix in [
        "",
        "An error occurred while importing the configuration from XML ",
    ] {
        let line = format!("[ERROR] {prefix}{diagnostic}\n");
        assert_eq!(
            locate_output_file(&map, &root, line.as_bytes())
                .expect("observed XDTO address")
                .path
                .as_str(),
            path
        );
    }
    for line in [
        format!("[WARN] {diagnostic}"),
        format!("[ERROR] unknown wrapper {diagnostic}"),
        format!(
            "[ERROR] {}",
            diagnostic.replace(". Error converting XDTO data: ", ". Unexpected text: ")
        ),
    ] {
        assert!(locate_output_file(&map, &root, line.as_bytes()).is_none());
    }
}
