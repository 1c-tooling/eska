//! Same-spelled parameters remain isolated, including current unsaved declaration changes.

use super::*;

/// Include-declaration, either name boundary and overlays share native semantic identity.
#[test]
fn current_reference_locations_follow_scope_edges_and_editor_bytes_in_both_locales() {
    let (_directory, root) = support::project(BODY, false);
    let path = root.join("src").join(MODULE);
    let uri = url::Url::from_file_path(&path).unwrap();
    let at = BODY.find("Аргумент").unwrap();
    let changed = format!("\u{feff}\r\n{}", BODY.trim_start_matches('\u{feff}'))
        .replace("Аргумент", "Параметр");
    for declaration in [false, true] {
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            let init = client.ask(&initialize());
            assert_eq!(init["result"]["capabilities"]["referencesProvider"], true);
            assert_eq!(
                init["result"]["capabilities"]["experimental"]["eska"]["references"],
                json!({"scope":"current-ready-bsl-inputs","incompleteResults":"location-context-or-error","maxFiles":200,"maxLocations":10000})
            );
            client.send(&notification("initialized"));
            let first = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "0",
                position(BODY, at),
                declaration,
            ));
            assert_eq!(
                first["result"].as_array().unwrap().len(),
                usize::from(declaration) + 1
            );
            for hit in first["result"].as_array().unwrap() {
                verify(hit, MODULE, BODY, BODY, "1", "Аргумент");
                assert_eq!(hit["eska"]["resources"]["metadata_source_bytes"], 0);
            }
            let edge = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "1",
                position(BODY, at + "Аргумент".len()),
                declaration,
            ));
            assert_eq!(edge["result"][0]["range"], first["result"][0]["range"]);
            client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":uri.as_str(),"version":1,"languageId":"bsl","text":changed}}}));
            let stale = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "1",
                position(BODY, at),
                declaration,
            ));
            assert_eq!(stale["error"]["data"]["kind"], "stale_revision");
            let changed_at = changed.find("Параметр").unwrap();
            let current = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "2",
                position(&changed, changed_at),
                declaration,
            ));
            assert_eq!(
                current["result"].as_array().unwrap().len(),
                usize::from(declaration) + 1
            );
            for hit in current["result"].as_array().unwrap() {
                verify(hit, MODULE, &changed, &changed, "2", "Параметр");
            }
            client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
            let closed = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "3",
                position(BODY, at),
                declaration,
            ));
            for hit in closed["result"].as_array().unwrap() {
                verify(hit, MODULE, BODY, BODY, "3", "Аргумент");
            }
            let trace = vec![init, first, edge, stale, current, closed];
            if let Some(previous) = &previous {
                assert_eq!(&trace, previous);
            }
            previous = Some(trace);
            client.finish();
        }
    }
    assert_eq!(fs::read_to_string(path).unwrap(), BODY);
}

/// Resolved zero and missing anchors keep incomplete status until the current inputs are all ready.
#[test]
fn empty_and_missing_references_do_not_hide_incomplete_inputs() {
    let body = "Процедура Пример(Аргумент)\nКонецПроцедуры\n";
    let (_directory, root) = support::project(body, false);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let at = body.find("Аргумент").unwrap();
        let incomplete = client.ask(&pull(&root, MODULE, "src", "0", position(body, at), false));
        assert_eq!(incomplete["error"]["data"]["kind"], "references_incomplete");
        assert_eq!(
            incomplete["error"]["data"]["details"]["search"]["files_scanned"],
            1
        );
        for (revision, source) in [
            ("1", "configuration.yaml"),
            ("2", "CommonModules/Tools/object.yaml"),
        ] {
            assert!(
                client
                    .ask(&request(
                        json!("owner"),
                        "eska/metadata/read",
                        json!({"snapshot":snapshot(revision),"source":source})
                    ))
                    .get("result")
                    .is_some()
            );
        }
        assert_eq!(
            client.ask(&pull(&root, MODULE, "src", "3", position(body, at), false))["result"],
            json!([])
        );
        assert_eq!(
            client.ask(&pull(
                &root,
                MODULE,
                "src",
                "3",
                json!({"line":2,"character":0}),
                false
            ))["result"],
            Value::Null
        );
        client.finish();
    }
}

/// Native non-enumerable receiver members never claim zero references, even for ready current BSL.
#[test]
fn unsupported_platform_member_references_return_an_explicit_scope_error() {
    let body = "Локальная=Новый Массив;Локальная.Вставить(1,2);\n";
    let (_directory, root) = support::project(body, false);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let reply = client.ask(&pull(
            &root,
            MODULE,
            "src",
            "0",
            position(body, body.find("Вставить").unwrap()),
            false,
        ));
        assert_eq!(
            reply["error"]["data"]["kind"],
            "references_symbol_unsupported"
        );
        assert_eq!(
            reply["error"]["data"]["details"]["category"],
            "unknown_scope"
        );
        client.finish();
    }
}

/// The native positional anchor and wire ranges retain their own newline/UTF-16 coordinate rules.
#[test]
fn reference_ranges_keep_crlf_lf_and_lone_cr_coordinates() {
    for ending in ["\r\n", "\n", "\r"] {
        let body = BODY.replace("\r\n", ending);
        let (_directory, root) = support::project(&body, false);
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            client.ask(&initialize());
            client.send(&notification("initialized"));
            let reply = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "0",
                json!({"line":0,"character":18}),
                false,
            ));
            let hits = reply["result"].as_array().unwrap();
            assert_eq!(hits.len(), 1, "{reply}");
            assert_eq!(
                hits[0]["range"],
                json!({"start":{"line":1,"character":15},"end":{"line":1,"character":23}})
            );
            assert_eq!(hits[0]["eska"]["target"]["source_sha256"], hash(&body));
            client.finish();
        }
    }
}
