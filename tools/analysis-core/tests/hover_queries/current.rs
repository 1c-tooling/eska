//! Local hovers follow current BSL bytes and restore saved cards after close.

use super::*;

/// Both markup formats preserve current docs/types/ranges while machine output remains locale-free.
#[test]
fn current_cards_and_identifier_edges_follow_editor_authority_in_both_formats_and_locales() {
    let (_directory, root) = support::project(BODY, false);
    let path = root.join("src").join(MODULE);
    let uri = url::Url::from_file_path(&path).unwrap();
    let at = BODY.rfind("Значение").unwrap();
    let changed = format!(
        "\u{feff}// Current Я😀\r\n{}",
        BODY.trim_start_matches('\u{feff}')
    )
    .replace("Возврат 1;", "Возврат \"новый тип\";")
    .replace("Текущая", "Изменённая");
    for format in ["plaintext", "markdown"] {
        let mut previous = None;
        for (lang, method, type_label, number, string) in [
            ("en-US", "Method", "Return type", "Number", "String"),
            ("ru-RU", "Метод", "Возвращаемый тип", "Число", "Строка"),
        ] {
            let mut client = Client::with_options(&root, lang, &[]);
            let initialized = client.ask(&initialize_hover(json!(["unknown", format])));
            assert_eq!(initialized["result"]["capabilities"]["hoverProvider"], true);
            assert_eq!(
                initialized["result"]["capabilities"]["experimental"]["eska"]["hover"],
                json!({"scope":"resolved-bsl-symbol","contentFormat":format,"incompleteResults":"error"})
            );
            client.send(&notification("initialized"));
            let first = client.ask(&pull(&root, MODULE, "src", "0", position(BODY, at)));
            verify(&first["result"], BODY, MODULE, "1", at, "Значение");
            let contents = first["result"]["contents"]["value"].as_str().unwrap();
            assert!(
                contents.contains(method)
                    && contents.contains(type_label)
                    && contents.contains(number)
            );
            assert_eq!(first["result"]["contents"]["kind"], format);
            assert_eq!(
                first["result"]["eska"]["resources"]["metadata_source_bytes"],
                0
            );
            let edge = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "1",
                position(BODY, at + "Значение".len()),
            ));
            assert_eq!(edge["result"]["contents"], first["result"]["contents"]);
            assert_eq!(edge["result"]["range"], first["result"]["range"]);
            assert_eq!(
                edge["result"]["eska"]["position_byte"],
                at + "Значение".len()
            );
            client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":uri.as_str(),"version":1,"languageId":"bsl","text":changed}}}));
            let stale = client.ask(&pull(&root, MODULE, "src", "1", position(BODY, at)));
            assert_eq!(stale["error"]["data"]["kind"], "stale_revision");
            let changed_at = changed.rfind("Значение").unwrap();
            let current = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "2",
                position(&changed, changed_at),
            ));
            verify(
                &current["result"],
                &changed,
                MODULE,
                "2",
                changed_at,
                "Значение",
            );
            let value = current["result"]["contents"]["value"].as_str().unwrap();
            assert!(value.contains(string) && value.contains("Изменённая"));
            assert!(!value.contains("Текущая"));
            client.ask(
                &json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
                "textDocument":{"uri":uri.as_str()}}}),
            );
            let closed = client.ask(&pull(&root, MODULE, "src", "3", position(BODY, at)));
            verify(&closed["result"], BODY, MODULE, "3", at, "Значение");
            assert_eq!(closed["result"]["contents"], first["result"]["contents"]);
            let trace = vec![first, edge, stale, current, closed]
                .into_iter()
                .map(machine)
                .collect::<Vec<_>>();
            if let Some(previous) = &previous {
                assert_eq!(&trace, previous);
            }
            previous = Some(trace);
            client.finish();
        }
    }
    assert_eq!(fs::read_to_string(path).unwrap(), BODY);
}

/// Missing cards stay explicitly incomplete until all current inputs are ready, then return null.
#[test]
fn unresolved_and_non_symbol_positions_do_not_claim_complete_empty_analysis() {
    let (_directory, root) = support::project(BODY, false);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        let initialized = client.ask(&initialize());
        assert_eq!(
            initialized["result"]["capabilities"]["experimental"]["eska"]["hover"]["contentFormat"],
            "plaintext"
        );
        client.send(&notification("initialized"));
        let mut query = pull(&root, MODULE, "src", "0", json!({"line":0,"character":3}));
        let incomplete = client.ask(&query);
        assert_eq!(incomplete["error"]["data"]["kind"], "hover_incomplete");
        assert_eq!(
            incomplete["error"]["data"]["details"]["snapshot"],
            snapshot("1")
        );
        assert_eq!(
            incomplete["error"]["data"]["details"]["resources"]["metadata_source_bytes"],
            0
        );
        for (revision, source) in [
            ("1", "configuration.yaml"),
            ("2", "CommonModules/Tools/object.yaml"),
        ] {
            let loaded = client.ask(&request(
                json!("metadata"),
                "eska/metadata/read",
                json!({"snapshot":snapshot(revision),"source":source}),
            ));
            assert!(loaded.get("result").is_some(), "{loaded}");
        }
        query["params"]["eska"]["snapshot"] = snapshot("3");
        assert_eq!(client.ask(&query)["result"], Value::Null);
        query["params"]["position"] = json!({"line":5,"character":4});
        assert_eq!(
            client.ask(&query)["error"]["data"]["kind"],
            "position_invalid"
        );
        client.finish();
    }
}

/// LSP rows use logical line breaks even when the kernel's internal index counts only LF.
#[test]
fn hover_wire_ranges_keep_current_crlf_lf_and_lone_cr_coordinates() {
    let source = "\u{feff}Функция Значение() Экспорт\r\nВозврат 1;\r\nКонецФункции\r\nЯ=\"😀\";Значение();\r\n";
    for ending in ["\r\n", "\n", "\r"] {
        let body = source.replace("\r\n", ending);
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
                json!({"line":3,"character":7}),
            ));
            assert!(reply.get("result").is_some(), "{reply}");
            assert_eq!(
                reply["result"]["range"],
                json!({"start":{"line":3,"character":7},
                "end":{"line":3,"character":15}})
            );
            assert_eq!(reply["result"]["eska"]["source_sha256"], hash(&body));
            assert_eq!(
                reply["result"]["eska"]["start_byte"],
                body.rfind("Значение").unwrap()
            );
            client.finish();
        }
    }
}
