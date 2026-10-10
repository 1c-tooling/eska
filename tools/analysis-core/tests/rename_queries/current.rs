//! Current native locals can be previewed and applied by a client without changing saved bytes.

use super::*;

/// Parameter, explicit-local and implicit-local plans keep their scope, editor version and policy.
#[test]
fn rename_previews_are_versioned_and_locale_independent_for_each_local_binding() {
    let (_directory, root, reference) = project(BODY, false);
    let saved = fs::read(root.join("src").join(MODULE)).unwrap();
    for (name, count) in [("Аргумент", 2), ("Локальная", 3), ("Неявная", 2)]
    {
        let mut previous = None;
        for locale in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(
                &root,
                locale,
                &["--accepted-base-sha256", &reference.sha256],
            );
            let initialized = client.ask(&initialize_rename(true));
            assert_eq!(
                initialized["result"]["capabilities"]["renameProvider"],
                json!({"prepareProvider":true,"workDoneProgress":false})
            );
            client.send(&notification("initialized"));
            let opened = open(&mut client, &root, BODY, 7);
            let at = BODY.find(name).unwrap();
            let prepare = client.ask(&query(&root, BODY, at + name.len(), "1", None));
            assert_eq!(prepare["result"]["placeholder"], name);
            assert_eq!(
                prepare["result"]["range"],
                json!({"start":position(BODY,at),"end":position(BODY,at+name.len())})
            );
            let plan = client.ask(&query(&root, BODY, at, "1", Some("АктуальноеИмя")));
            let result = &plan["result"];
            let context = &result["eska"];
            assert_eq!(context["snapshot"], snapshot("1"));
            assert_eq!(context["source_sha256"], hash(BODY));
            assert_eq!(context["document_version"], 7);
            assert_eq!(context["scope_verified"], true);
            assert_eq!(context["reference_search_complete"], true);
            assert_eq!(context["permission"]["baseline"], reference.sha256);
            assert_eq!(context["coverage"]["semantic_complete"], false);
            assert_eq!(context["coverage"]["metadata_models_complete"], false);
            assert_eq!(context["resources"], opened["params"]["resources"]);
            assert_eq!(result["documentChanges"][0]["textDocument"]["version"], 7);
            assert_eq!(
                result["documentChanges"][0]["edits"]
                    .as_array()
                    .unwrap()
                    .len(),
                count
            );
            let changed = apply(BODY, result);
            assert!(!parser::parse(&changed).has_errors());
            assert!(changed.contains("Функция Другой(Аргумент)\r\nВозврат Аргумент;"));
            if let Some(before) = &previous {
                assert_eq!(&(prepare, plan), before);
            } else {
                previous = Some((prepare, plan));
            }
            client.finish();
        }
    }
    assert_eq!(fs::read(root.join("src").join(MODULE)).unwrap(), saved);
}

/// The client applies a plan through didChange; stale requests and a closed document cannot reuse it.
#[test]
fn client_applied_rename_follows_new_hash_revision_and_close() {
    let (_directory, root, reference) = project(BODY, false);
    for locale in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(
            &root,
            locale,
            &["--accepted-base-sha256", &reference.sha256],
        );
        client.ask(&initialize_rename(false));
        client.send(&notification("initialized"));
        open(&mut client, &root, BODY, 1);
        let plan = client.ask(&query(
            &root,
            BODY,
            BODY.find("Аргумент").unwrap(),
            "1",
            Some("ТекущийПараметр"),
        ));
        let changed = apply(BODY, &plan["result"]);
        let uri = url::Url::from_file_path(root.join("src").join(MODULE)).unwrap();
        let updated = client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri.as_str(),"version":2},"contentChanges":[{"text":changed}]}}),
        );
        assert_eq!(updated["params"]["snapshot"], snapshot("2"));
        let at = changed.find("ТекущийПараметр").unwrap();
        let prepare = client.ask(&query(&root, &changed, at, "2", None));
        assert_eq!(prepare["result"]["placeholder"], "ТекущийПараметр");
        assert_eq!(prepare["result"]["eska"]["source_sha256"], hash(&changed));
        assert_eq!(
            client.ask(&query(&root, &changed, at, "1", Some("Свежий")))["error"]["data"]["kind"],
            "stale_revision"
        );
        let noop = client.ask(&query(&root, &changed, at, "2", Some("ТекущийПараметр")));
        assert_eq!(noop["result"]["eska"]["unchanged"], true);
        assert_eq!(noop["result"]["documentChanges"][0]["edits"], json!([]));
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
        assert_eq!(
            client.ask(&query(
                &root,
                BODY,
                BODY.find("Аргумент").unwrap(),
                "3",
                None
            ))["error"]["data"]["kind"],
            "rename_document_not_open"
        );
        client.finish();
    }
    assert_eq!(
        fs::read_to_string(root.join("src").join(MODULE)).unwrap(),
        BODY
    );
}

/// Native LF-only anchors do not replace transport newline rules for prepare and edit coordinates.
#[test]
fn local_rename_coordinates_preserve_bom_and_each_supported_line_ending() {
    let (_directory, root, reference) = project(BODY, false);
    for locale in ["en-US", "ru-RU"] {
        for eol in ["\n", "\r\n", "\r"] {
            let text =
                format!("\u{feff}Процедура Тест(Арг){eol}Сообщить(Арг);{eol}КонецПроцедуры{eol}");
            let mut client = Client::with_options(
                &root,
                locale,
                &["--accepted-base-sha256", &reference.sha256],
            );
            client.ask(&initialize_rename(true));
            client.send(&notification("initialized"));
            open(&mut client, &root, &text, 1);
            let mut query = query(&root, BODY, 0, "1", Some("Параметр"));
            query["params"]["position"] = json!({"line":1,"character":9});
            let plan = client.ask(&query);
            assert_eq!(plan["result"]["eska"]["source_sha256"], hash(&text));
            let edits = plan["result"]["documentChanges"][0]["edits"]
                .as_array()
                .unwrap();
            assert_eq!(edits.len(), 2);
            assert_eq!(
                edits[0]["range"],
                json!({"start":{"line":0,"character":16},"end":{"line":0,"character":19}})
            );
            assert_eq!(
                edits[1]["range"],
                json!({"start":{"line":1,"character":9},"end":{"line":1,"character":12}})
            );
            client.finish();
        }
    }
}
