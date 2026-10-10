//! Negotiation and rejected rename requests leave the current source generation untouched.

use super::*;

/// Neither an unpinned installation nor an unversioned client receives rename authority.
#[test]
fn rename_provider_requires_pin_and_versioned_edits_and_negotiates_prepare_shape() {
    let (_directory, root, reference) = project(BODY, false);
    for locale in ["en-US", "ru-RU"] {
        for prepare in [false, true] {
            let mut client = Client::with_options(
                &root,
                locale,
                &["--accepted-base-sha256", &reference.sha256],
            );
            let ready = client.ask(&initialize_rename(prepare));
            assert_eq!(
                ready["result"]["capabilities"]["renameProvider"],
                if prepare {
                    json!({"prepareProvider":true,"workDoneProgress":false})
                } else {
                    json!(true)
                }
            );
            assert_eq!(
                ready["result"]["capabilities"]["experimental"]["eska"]["localRename"]["scope"],
                "method-local-bsl"
            );
            client.send(&notification("initialized"));
            client.finish();
        }
        for pinned in [false, true] {
            let options = if pinned {
                vec!["--accepted-base-sha256", reference.sha256.as_str()]
            } else {
                Vec::new()
            };
            let mut client = Client::with_options(&root, locale, &options);
            let ready = client.ask(&initialize());
            assert!(
                ready["result"]["capabilities"]
                    .get("renameProvider")
                    .is_none()
            );
            client.send(&notification("initialized"));
            assert_eq!(
                client.ask(&query(
                    &root,
                    BODY,
                    BODY.find("Аргумент").unwrap(),
                    "0",
                    Some("Свежий")
                ))["error"]["data"]["kind"],
                "rename_plans_disabled"
            );
            client.finish();
        }
    }
}

/// Native invalid/captured names, nonlocal symbols and malformed standard params fail explicitly.
#[test]
fn rename_refusals_and_standard_param_guards_are_locale_independent() {
    let (_directory, root, reference) = project(BODY, false);
    let mut previous = None;
    for locale in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(
            &root,
            locale,
            &["--accepted-base-sha256", &reference.sha256],
        );
        client.ask(&initialize_rename(true));
        client.send(&notification("initialized"));
        let opened = open(&mut client, &root, BODY, 1);
        let at = BODY.find("Аргумент").unwrap();
        let mut trace = Vec::new();
        for name in ["", "Если", "A.B", "A=1", "A\n", "😀"] {
            let refused = client.ask(&query(&root, BODY, at, "1", Some(name)));
            assert_eq!(refused["error"]["data"]["kind"], "rename_name_invalid");
            trace.push(refused);
        }
        for name in ["локальная", "ЗНАЧЕНИЕ", "Неявная"] {
            let refused = client.ask(&query(&root, BODY, at, "1", Some(name)));
            assert_eq!(refused["error"]["data"]["kind"], "rename_name_conflict");
            trace.push(refused);
        }
        let refused = client.ask(&query(&root, BODY, at, "1", Some(&"A".repeat(1025))));
        assert_eq!(refused["error"]["data"]["kind"], "rename_name_too_large");
        assert_eq!(refused["error"]["data"]["details"]["limit"], 1024);
        trace.push(refused);
        let unsupported = client.ask(&query(
            &root,
            BODY,
            BODY.find("Значение").unwrap(),
            "1",
            None,
        ));
        assert_eq!(
            unsupported["error"]["data"]["kind"],
            "rename_symbol_unsupported"
        );
        trace.push(unsupported);
        let mut malformed = query(&root, BODY, at, "1", Some("Свежий"));
        malformed["params"]
            .as_object_mut()
            .unwrap()
            .remove("newName");
        assert_eq!(
            client.ask(&malformed)["error"]["data"]["kind"],
            "rpc_invalid_params"
        );
        for method in ["textDocument/prepareRename", "textDocument/rename"] {
            let mut malformed = query(
                &root,
                BODY,
                at,
                "1",
                if method.ends_with("/rename") {
                    Some("Свежий")
                } else {
                    None
                },
            );
            malformed["params"]["partialResultToken"] = json!(1);
            assert_eq!(
                client.ask(&malformed)["error"]["data"]["kind"],
                "rpc_invalid_params"
            );
            malformed["params"]
                .as_object_mut()
                .unwrap()
                .remove("partialResultToken");
            malformed["params"]["position"] = json!({"line":2,"character":4});
            assert_eq!(
                client.ask(&malformed)["error"]["data"]["kind"],
                "position_invalid"
            );
        }
        let state = client.ask(&request(
            json!("state"),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("1"),"limit":1}),
        ));
        assert_eq!(state["result"]["resources"], opened["params"]["resources"]);
        if let Some(before) = &previous {
            assert_eq!(&trace, before);
        } else {
            previous = Some(trace);
        }
        client.finish();
    }
}

/// Pending/closed input and zero/overflow deadlines never reach lazy source or accepted-base IO.
#[test]
fn rename_preflights_reject_unread_closed_and_stopped_requests_without_source_loading() {
    let (_directory, root, reference) = project(BODY, false);
    for locale in ["en-US", "ru-RU"] {
        for timeout in [None, Some("0"), Some("18446744073709551615")] {
            let mut options = vec!["--accepted-base-sha256", reference.sha256.as_str()];
            if let Some(timeout) = timeout {
                options.extend(["--timeout-seconds", timeout]);
            }
            let mut client = Client::with_options(&root, locale, &options);
            client.ask(&initialize_rename(true));
            client.send(&notification("initialized"));
            for replacement in [None, Some("Свежий")] {
                let refused = client.ask(&query(
                    &root,
                    BODY,
                    BODY.find("Аргумент").unwrap(),
                    "0",
                    replacement,
                ));
                assert_eq!(
                    refused["error"]["data"]["kind"],
                    match timeout {
                        None => "module_unavailable",
                        Some("0") => "analysis_deadline_exceeded",
                        Some(_) => "analysis_timeout_out_of_range",
                    }
                );
            }
            let state = client.ask(&request(
                json!("state"),
                "eska/metadata/owners",
                json!({"snapshot":snapshot("0"),"limit":1}),
            ));
            assert_eq!(state["result"]["resources"]["module_text_bytes"], 0);
            assert_eq!(state["result"]["resources"]["metadata_source_bytes"], 0);
            client.finish();
        }
    }
}

/// Invalid frozen analysis settings remove rename providers; recovered syntax is not edit eligibility.
#[test]
fn rename_obeys_analysis_settings_and_rejects_syntactically_invalid_open_text() {
    let (_directory, root, reference) = project(BODY, false);
    for locale in ["en-US", "ru-RU"] {
        fs::write(root.join("bsl-analyzer.toml"), "invalid = [").unwrap();
        let mut client = Client::with_options(
            &root,
            locale,
            &["--accepted-base-sha256", &reference.sha256],
        );
        let ready = client.ask(&initialize_rename(true));
        assert!(
            ready["result"]["capabilities"]
                .get("renameProvider")
                .is_none()
        );
        client.send(&notification("initialized"));
        open(&mut client, &root, BODY, 1);
        assert!(
            client
                .ask(&query(
                    &root,
                    BODY,
                    BODY.find("Аргумент").unwrap(),
                    "1",
                    None
                ))
                .get("error")
                .is_some()
        );
        client.finish();
        fs::remove_file(root.join("bsl-analyzer.toml")).unwrap();
        let mut client = Client::with_options(
            &root,
            locale,
            &["--accepted-base-sha256", &reference.sha256],
        );
        client.ask(&initialize_rename(true));
        client.send(&notification("initialized"));
        open(&mut client, &root, "Функция Ошибка(", 1);
        let mut query = query(&root, BODY, 0, "1", None);
        query["params"]["position"] = json!({"line":0,"character":8});
        assert_eq!(
            client.ask(&query)["error"]["data"]["kind"],
            "rename_syntax_invalid"
        );
        client.finish();
    }
}
