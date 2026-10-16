//! Metadata-only findings retain exact Unicode header coordinates in real IDE and checker processes.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::*;

const ACTUAL_HEADER: &str = "\u{feff}\r\n\r\n#Область Проверка\r\n// Я😀\r\n&НаКлиенте\r\nПроцедура SavedHandler()\r\nКонецПроцедуры\r\n#КонецОбласти\r\n";
const EMOJI_HEADER: &str =
    "\u{feff}//Я😀Привет\r\n&НаКлиенте\r\nПроцедура SavedHandler()\r\nКонецПроцедуры\r\n";
const CODE: &str = "WrongDataPathForFormElements";

/// Keep the actual metadata-only rule enabled without depending on the default rule profile.
fn invalid_path_settings(root: &Path) {
    let text = ide::all_diagnostic_codes().fold(
        "[diagnostics.parameters]\n".to_owned(),
        |mut text, code| {
            writeln!(text, "{}={}", code.as_str(), code.as_str() == CODE).unwrap();
            text
        },
    );
    fs::write(root.join("bsl-analyzer.toml"), text).unwrap();
}

/// Use the existing imported managed form and change only its declared module and current dialog.
fn invalid_path_project(body: &str) -> (support::TestDirectory, std::path::PathBuf) {
    let (directory, root) = support::project("// Shared module.\n", false);
    support::form_project::add_forms(&root, &directory.0.join("native"), false);
    let source = root.join("src-with-forms");
    let dialog = fs::read_to_string(source.join(DIALOG)).unwrap();
    assert_eq!(
        dialog.matches("<DataPath>Object.Code</DataPath>").count(),
        1
    );
    fs::write(
        source.join(DIALOG),
        dialog.replace(
            "<DataPath>Object.Code</DataPath>",
            "<DataPath>~Object.Missing</DataPath>",
        ),
    )
    .unwrap();
    fs::write(source.join(MODULE), body).unwrap();
    invalid_path_settings(&root);
    (directory, root)
}

/// Compare fingerprints against the actual verbatim fixture bytes, including BOM and CRLF.
fn source_hash(body: &str) -> String {
    Sha256::digest(body.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut hash, byte| {
            write!(hash, "{byte:02x}").unwrap();
            hash
        })
}

/// Derive wire positions from Unicode text independently of both native and SDK coordinate helpers.
fn position(body: &str, byte: usize, utf16: bool) -> Value {
    let prefix = &body[..byte];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let tail = prefix.rsplit('\n').next().unwrap();
    let character = if utf16 {
        tail.encode_utf16().count()
    } else {
        tail.chars().count()
    };
    json!({"line":line,"character":character})
}

/// A header anchor covers nine Unicode scalars, rather than nine arbitrary UTF-8 bytes.
fn assert_finding(body: &str, item: &Value, finding: &Value) {
    let end = body.chars().take(9).map(char::len_utf8).sum::<usize>();
    assert!(body.is_char_boundary(end));
    assert_eq!(item["code"], CODE);
    assert_eq!(finding["code"], CODE);
    assert_eq!(item["severity"], 1);
    assert_eq!(finding["severity"], item["data"]["original_severity"]);
    assert_eq!(item["data"]["start_byte"], 0);
    assert_eq!(item["data"]["end_byte"], end);
    assert_eq!(finding["start_byte"], 0);
    assert_eq!(finding["end_byte"], end);
    assert_eq!(
        item["range"],
        json!({"start":position(body,0,true),"end":position(body,end,true)})
    );
    let batch_end = position(body, end, false);
    assert_eq!(finding["start_line"], 0);
    assert_eq!(finding["start_column"], 0);
    assert_eq!(finding["end_line"], batch_end["line"]);
    assert_eq!(finding["end_column"], batch_end["character"]);
    if body == EMOJI_HEADER {
        assert_ne!(batch_end["character"], item["range"]["end"]["character"]);
    }
}

/// A real form finding survives the formerly split Cyrillic/emoji endpoints and keeps RU/EN parity.
#[test]
fn metadata_header_findings_keep_valid_unicode_ranges_in_ide_and_checker() {
    for body in [ACTUAL_HEADER, EMOJI_HEADER] {
        assert!(!body.is_char_boundary(9));
        let (_directory, root) = invalid_path_project(body);
        let source = root.join("src-with-forms");
        let saved_dialog = fs::read(source.join(DIALOG)).unwrap();
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let output = Command::new(env!("CARGO_BIN_EXE_eska-check"))
                .current_dir(&root)
                .env("PATH", "")
                .args(["--lang", lang, "--profile", "semantic", "--format", "json"])
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(1),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
            let check: Value = serde_json::from_slice(&output.stdout).unwrap();
            let batch = &check["data"]["report"];
            assert_eq!(batch["coverage"]["semantic_profile_complete"], true);
            let checked = batch["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|file| file["source"] == MODULE)
                .unwrap();
            let findings = checked["findings"].as_array().unwrap();
            assert_eq!(findings.len(), 1);
            let mut client = Client::with_options(&root, lang, &["--profile", "semantic"]);
            client.ask(&initialize());
            client.send(&notification("initialized"));
            let ready = client.ask(&diagnostic(&source, &snapshot("0")));
            assert!(ready.get("error").is_none(), "{ready}");
            let result = &ready["result"];
            let items = result["items"].as_array().unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(result["eska"]["source_sha256"], source_hash(body));
            assert_eq!(checked["source_sha256"], result["eska"]["source_sha256"]);
            assert_eq!(
                check["data"]["settings_sha256"],
                result["eska"]["config_sha256"]
            );
            assert_eq!(
                result["eska"]["coverage"]["semantic_profile_complete"],
                true
            );
            assert_finding(body, &items[0], &findings[0]);
            if let Some(previous) = &previous {
                assert_eq!(&check, previous);
            }
            previous = Some(check);
            client.finish();
        }
        assert_eq!(fs::read(source.join(MODULE)).unwrap(), body.as_bytes());
        assert_eq!(fs::read(source.join(DIALOG)).unwrap(), saved_dialog);
        assert!(!root.join(".eska").exists());
    }
}
