//! Both standard rename methods retain editor input after managed cancellation and ID reuse.

use super::*;

/// Native work shares the reader's stop token; an idle cancellation cannot poison a later request.
#[test]
fn cancelled_local_rename_requests_preserve_input_and_recover_in_both_locales() {
    let (_directory, root, reference) = project(BODY, false);
    let large = format!(
        "Procedure Test(Argument)\n{}EndProcedure\n",
        "Value=Argument;\n".repeat(90_000)
    );
    for (locale, id) in [("en-US", json!(-9)), ("ru-RU", json!("переименование"))] {
        for replacement in [None, Some("Свежий")] {
            let mut client = Client::with_options(
                &root,
                locale,
                &["--accepted-base-sha256", &reference.sha256],
            );
            client.ask(&initialize_rename(true));
            client.send(&notification("initialized"));
            let opened = open(&mut client, &root, &large, 1);
            let mut query = query(
                &root,
                &large,
                large.find("Argument").unwrap(),
                "1",
                replacement,
            );
            query["id"] = id.clone();
            client.send(&query);
            client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}}));
            let cancelled = client.receive();
            assert_eq!(cancelled["id"], id);
            assert_eq!(cancelled["error"]["code"], -32800);
            assert_eq!(cancelled["error"]["data"]["details"], snapshot("1"));
            let state = client.ask(&request(
                id.clone(),
                "eska/metadata/owners",
                json!({"snapshot":snapshot("1"),"limit":1}),
            ));
            assert_eq!(state["result"]["resources"], opened["params"]["resources"]);
            let uri = url::Url::from_file_path(root.join("src").join(MODULE)).unwrap();
            client.ask(
                &json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                "textDocument":{"uri":uri.as_str(),"version":2},"contentChanges":[{"text":BODY}]}}),
            );
            client
                .send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":"reuse"}}));
            let mut next = super::query(
                &root,
                BODY,
                BODY.find("Аргумент").unwrap(),
                "2",
                replacement,
            );
            next["id"] = json!("reuse");
            let result = client.ask(&next);
            assert!(result.get("result").is_some(), "{result}");
            assert_eq!(result["result"]["eska"]["snapshot"], snapshot("2"));
            client.finish();
        }
    }
}
