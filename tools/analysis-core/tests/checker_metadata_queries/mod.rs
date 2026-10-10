//! Actual IDE and CI processes expose identical current metadata findings through independent entry points.

use super::*;
use support::metadata::{OWNER, text as current_text};
use url::Url;

/// Comparing wire output verifies shared causes and ranges without comparing either tool to its own mapper.
#[test]
fn checker_and_ide_metadata_findings_match_for_each_locale() {
    for parse_issue in [false, true] {
        let (_directory, root) = support::project("// Saved syntax is complete.\n", false);
        let file = root.join("src").join(OWNER);
        let text = current_text(&root, parse_issue);
        fs::write(&file, &text).unwrap();
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let output = Command::new(env!("CARGO_BIN_EXE_eska-check"))
                .current_dir(&root)
                .env("PATH", "")
                .args(["--lang", lang, "--profile", "syntax", "--format", "json"])
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stderr.is_empty());
            let check: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(check["schema_version"], 1);
            let issues = check["data"]["report"]["metadata_issues"]
                .as_array()
                .unwrap();
            assert_eq!(issues.len(), 1);
            let mut client = Client::with_options(&root, lang, &[]);
            assert_eq!(
                client.ask(&initialize())["result"]["capabilities"]["positionEncoding"],
                "utf-16"
            );
            client.send(&notification("initialized"));
            let response = client.ask(&request(
                json!(20),
                "textDocument/diagnostic",
                json!({"textDocument":{"uri":Url::from_file_path(&file).unwrap().as_str()}}),
            ));
            assert!(response.get("error").is_none(), "{response}");
            let result = &response["result"];
            assert_eq!(issues[0]["source"], result["eska"]["source"]);
            assert_eq!(issues[0]["source_sha256"], result["eska"]["source_sha256"]);
            let mut items = result["items"].clone();
            for item in items.as_array_mut().unwrap() {
                let fields = item.as_object_mut().unwrap();
                assert_eq!(fields.remove("source").unwrap(), "eska-analyzer");
                assert!(
                    !fields
                        .remove("message")
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .is_empty()
                );
            }
            assert_eq!(issues[0]["findings"], items);
            assert_eq!(items.as_array().unwrap().len(), 1);
            if let Some(previous) = &previous {
                assert_eq!(&check, previous);
            }
            previous = Some(check);
            assert_eq!(fs::read_to_string(&file).unwrap(), text);
            client.finish();
        }
    }
}
