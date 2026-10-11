//! Current call binding chooses nested arguments and follows unsaved declarations without disk writes.

use super::*;

/// Provider negotiation is a wire contract, independent of the current signature's contents.
fn negotiated(initialized: &Value, format: &str, offsets: bool) {
    assert_eq!(
        initialized["result"]["capabilities"]["signatureHelpProvider"],
        json!({"triggerCharacters":["(",","],"retriggerCharacters":[")"],"workDoneProgress":false})
    );
    assert_eq!(
        initialized["result"]["capabilities"]["experimental"]["eska"]["signatureHelp"],
        json!({"scope":"current-bsl-call","documentationFormat":format,
            "parameterLabel":if offsets {"utf16-offsets"} else {"string"},
            "incompleteResults":"error","clientHints":"recompute",
            "inputPreparation":"direct-member-targets"})
    );
}

/// A retrigger's obsolete card is a client hint, never an input to native overload selection.
fn recomputed(client: &mut Client, mut query: Value) -> Value {
    query["params"]["context"] = json!({"triggerKind":3,"isRetrigger":true,
        "activeSignatureHelp":{"signatures":[{"label":"obsolete()"}],"activeParameter":99}});
    client.ask(&query)
}

/// Inner-call commas and commas inside strings belong to distinct native argument selections.
fn nested_arguments(client: &mut Client, root: &Path) -> (Value, Value) {
    let inner_at = BODY.rfind("Вызов(1)").unwrap() + "Вызов(".len();
    let inner = client.ask(&pull(root, MODULE, "src", "1", position(BODY, inner_at)));
    assert_eq!(inner["result"]["activeParameter"], 0);
    let string_at = BODY.rfind("x,y").unwrap() + 2;
    let string = client.ask(&pull(root, MODULE, "src", "1", position(BODY, string_at)));
    assert_eq!(string["result"]["activeParameter"], 1);
    (inner, string)
}

/// Display negotiation preserves candidate/argument selection and exact caller hashes across locales.
#[test]
fn current_signatures_follow_nested_calls_strings_and_editor_authority_in_both_locales() {
    let (_directory, root) = support::project(BODY, false);
    let path = root.join("src").join(MODULE);
    let uri = url::Url::from_file_path(&path).unwrap();
    let at = BODY.rfind(", );").unwrap() + 2;
    let changed = format!(
        "\u{feff}// Shifted Я😀\r\n{}",
        BODY.trim_start_matches('\u{feff}')
    )
    .replace("Другой =", "Актуальный =")
    .replace("Текущая", "Изменённая");
    for (format, offsets) in [("plaintext", false), ("markdown", true)] {
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            let initialized = client.ask(&initialize_signature(format, offsets));
            negotiated(&initialized, format, offsets);
            client.send(&notification("initialized"));
            let first = client.ask(&pull(&root, MODULE, "src", "0", position(BODY, at)));
            verify(&first["result"], BODY, MODULE, "1", at, offsets);
            assert_eq!(first["result"]["activeParameter"], 2);
            assert_eq!(first["result"]["signatures"][0]["activeParameter"], 2);
            assert_eq!(
                first["result"]["signatures"][0]["documentation"]["kind"],
                format
            );
            let label = first["result"]["signatures"][0]["label"].as_str().unwrap();
            assert!(label.contains("Другой") && label.contains('😀'));
            assert_eq!(
                first["result"]["eska"]["resources"]["metadata_source_bytes"],
                0
            );
            let (inner, string) = nested_arguments(&mut client, &root);
            let recomputed = recomputed(
                &mut client,
                pull(&root, MODULE, "src", "1", position(BODY, at)),
            );
            assert_eq!(recomputed, first);
            client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":uri.as_str(),"version":1,"languageId":"bsl","text":changed}}}));
            let stale = client.ask(&pull(&root, MODULE, "src", "1", position(BODY, at)));
            assert_eq!(stale["error"]["data"]["kind"], "stale_revision");
            let current_at = changed.rfind(", );").unwrap() + 2;
            let current = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "2",
                position(&changed, current_at),
            ));
            verify(
                &current["result"],
                &changed,
                MODULE,
                "2",
                current_at,
                offsets,
            );
            assert!(
                current["result"]["signatures"][0]["label"]
                    .as_str()
                    .unwrap()
                    .contains("Актуальный")
            );
            assert!(
                current["result"]["signatures"][0]["documentation"]["value"]
                    .as_str()
                    .unwrap()
                    .contains("Изменённая")
            );
            client.ask(
                &json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
                "textDocument":{"uri":uri.as_str()}}}),
            );
            let closed = client.ask(&pull(&root, MODULE, "src", "3", position(BODY, at)));
            verify(&closed["result"], BODY, MODULE, "3", at, offsets);
            assert_eq!(
                closed["result"]["signatures"],
                first["result"]["signatures"]
            );
            let trace = vec![
                initialized,
                first,
                inner,
                string,
                recomputed,
                stale,
                current,
                closed,
            ];
            if let Some(previous) = &previous {
                assert_eq!(&trace, previous);
            }
            previous = Some(trace);
            client.finish();
        }
    }
    assert_eq!(fs::read_to_string(path).unwrap(), BODY);
}

/// A missing call becomes standard null only once every current input is ready.
#[test]
fn absent_signature_stays_incomplete_until_current_metadata_is_ready() {
    let (_directory, root) = support::project(BODY, false);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        let initialized = client.ask(&initialize());
        assert!(
            initialized["result"]["capabilities"]["signatureHelpProvider"]
                .get("retriggerCharacters")
                .is_none()
        );
        client.send(&notification("initialized"));
        let mut query = pull(&root, MODULE, "src", "0", json!({"line":0,"character":3}));
        let incomplete = client.ask(&query);
        assert_eq!(
            incomplete["error"]["data"]["kind"],
            "signature_help_incomplete"
        );
        assert_eq!(
            incomplete["error"]["data"]["details"]["snapshot"],
            snapshot("1")
        );
        for (revision, source) in [
            ("1", "configuration.yaml"),
            ("2", "CommonModules/Tools/object.yaml"),
        ] {
            assert!(
                client
                    .ask(&request(
                        json!("metadata"),
                        "eska/metadata/read",
                        json!({"snapshot":snapshot(revision),"source":source})
                    ))
                    .get("result")
                    .is_some()
            );
        }
        query["params"]["eska"]["snapshot"] = snapshot("3");
        assert_eq!(client.ask(&query)["result"], Value::Null);
        query["params"]["position"] = json!({"line":0,"character":6});
        assert_eq!(
            client.ask(&query)["error"]["data"]["kind"],
            "position_invalid"
        );
        client.finish();
    }
}

/// Logical line endings affect only wire positions, never the generated signature's selection.
#[test]
fn signature_positions_keep_crlf_lf_and_lone_cr_coordinates() {
    // The pinned native lexer ends line comments on LF; this test isolates cursor conversion.
    let without_comments = BODY
        .replace("// Я😀", "")
        .replace("// Текущая **документация** [пример](command:run).", "");
    for ending in ["\r\n", "\n", "\r"] {
        let body = without_comments.replace("\r\n", ending);
        let (_directory, root) = support::project(&body, false);
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            client.ask(&initialize());
            client.send(&notification("initialized"));
            let at = body.rfind(", );").unwrap() + 2;
            let line = body.split(ending).nth(6).unwrap();
            let character = line[..line.rfind(", );").unwrap() + 2]
                .encode_utf16()
                .count();
            let reply = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "0",
                json!({"line":6,"character":character}),
            ));
            assert!(reply.get("error").is_none(), "{ending:?}: {reply}");
            verify(&reply["result"], &body, MODULE, "1", at, false);
            assert_eq!(reply["result"]["activeParameter"], 2);
            client.finish();
        }
    }
}

/// A zero-parameter call uses null only with the client's explicit no-active-parameter capability.
#[test]
fn zero_parameter_calls_negotiate_null_selection_without_empty_signature_arrays() {
    let body = "Procedure Test()\nEndProcedure\nTest();\n";
    let (_directory, root) = support::project(body, false);
    for supported in [false, true] {
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            client.ask(&if supported {
                initialize_signature("plaintext", true)
            } else {
                initialize()
            });
            client.send(&notification("initialized"));
            let reply = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "0",
                json!({"line":2,"character":5}),
            ));
            let result = &reply["result"];
            verify(
                result,
                body,
                MODULE,
                "1",
                body.rfind("Test(").unwrap() + 5,
                supported,
            );
            assert_eq!(result["signatures"].as_array().unwrap().len(), 1);
            assert_eq!(result["signatures"][0]["parameters"], json!([]));
            assert_eq!(
                result.get("activeParameter"),
                supported.then_some(&Value::Null)
            );
            assert_eq!(result["eska"]["active_parameter"], Value::Null);
            client.finish();
        }
    }
}

/// Platform constructor overloads and a typed receiver retain native order and parameter selection.
#[test]
fn constructors_and_typed_platform_methods_preserve_native_signatures() {
    let body = "Результат=Новый Массив(1, );\nКоллекция=Новый Массив;Коллекция.Вставить(1, );\n";
    let (_directory, root) = support::project(body, false);
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize_signature("plaintext", true));
        client.send(&notification("initialized"));
        let at = body.find(", );").unwrap() + 2;
        let constructor = client.ask(&pull(&root, MODULE, "src", "0", position(body, at)));
        verify(&constructor["result"], body, MODULE, "1", at, true);
        let signatures = constructor["result"]["signatures"].as_array().unwrap();
        assert_eq!(signatures.len(), 2);
        assert!(
            signatures[0]["label"]
                .as_str()
                .unwrap()
                .contains("По количеству элементов")
        );
        assert!(
            signatures[1]["label"]
                .as_str()
                .unwrap()
                .contains("На основании фиксированного массива")
        );
        assert_eq!(
            constructor["result"].get("activeParameter"),
            Some(&Value::Null)
        );
        let at = body.rfind(", );").unwrap() + 2;
        let method = client.ask(&pull(&root, MODULE, "src", "1", position(body, at)));
        verify(&method["result"], body, MODULE, "2", at, true);
        assert_eq!(
            constructor["result"]["eska"]["resources"]["metadata_source_bytes"],
            0
        );
        assert_eq!(
            method["result"]["eska"]["resources"]["metadata_source_bytes"],
            fs::metadata(root.join("src/CommonModules/Tools/object.yaml"))
                .unwrap()
                .len()
        );
        assert_eq!(method["result"]["activeParameter"], 1);
        assert_eq!(method["result"]["signatures"].as_array().unwrap().len(), 1);
        assert!(
            method["result"]["signatures"][0]["label"]
                .as_str()
                .unwrap()
                .starts_with("Массив.Вставить(")
        );
        let trace = (constructor, method);
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
}
