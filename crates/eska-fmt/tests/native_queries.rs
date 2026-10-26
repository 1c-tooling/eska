//! Opt-in SDBL acceptance uses T114's owned catalog/register/DCS fixture, never a user database.

use eska_project_core::formatting::{Language, Options, protocol::Request};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

/// Both revisions execute against identical seeded data and the fixture's original expected results.
#[test]
#[ignore = "requires ESKA_FMT_NATIVE_MATRIX, ESKA_FMT_NATIVE_PLATFORM and ESKA_FMT_NATIVE_DISTROBOX"]
fn native_matrix_preserves_virtual_tables_and_dcs_results() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let playground = root
        .parent()
        .unwrap()
        .join("eska-playground")
        .canonicalize()
        .unwrap();
    let fixture = Path::new(&std::env::var("ESKA_FMT_NATIVE_MATRIX").unwrap())
        .canonicalize()
        .unwrap();
    assert!(fixture.starts_with(playground));
    let owner: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.join("owner.json")).unwrap()).unwrap();
    assert_eq!(owner["task"], "T114");
    let input: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.join("generated-cases.json")).unwrap())
            .unwrap();
    let mut cases = Vec::new();
    for case in input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["application"] == "minimal")
    {
        let mut before = case.clone();
        let mut after = case.clone();
        before["name"] = format!("before_{}", case["name"].as_str().unwrap()).into();
        after["name"] = format!("after_{}", case["name"].as_str().unwrap()).into();
        let text = case["query"].as_str().unwrap();
        let mut request = Request {
            schema_version: 1,
            language: Language::Sdbl,
            text: text.into(),
            options: Options::default(),
            range: None,
        };
        let response =
            eska_fmt::format(&request).unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
        for edit in response.edits.iter().rev() {
            request
                .text
                .replace_range(edit.range.start..edit.range.end, &edit.text);
        }
        assert!(
            eska_fmt::format(&request).unwrap().edits.is_empty(),
            "{}",
            case["name"]
        );
        after["query"] = request.text.into();
        cases.extend([before, after]);
    }
    assert!(cases.len() >= 32);
    let mut child = Command::new("python3")
        .arg(root.join("tests/native_query_formatting.py"))
        .arg(fixture.join("matrix/build/matrix.cf"))
        .arg(std::env::var("ESKA_FMT_NATIVE_PLATFORM").unwrap())
        .arg(std::env::var("ESKA_FMT_NATIVE_DISTROBOX").unwrap())
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&serde_json::json!({"cases":cases})).unwrap())
        .unwrap();
    assert!(child.wait().unwrap().success());
}
