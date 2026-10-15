//! Real framed hovers prepare the exact current command descriptor before reading a local parameter.

use super::*;

const COMMAND_MODULE: &str = "CommonCommands/Run/CommandModule.bsl";
const COMMAND: &str = "CommonCommands/Run/command.yaml";
const COMMAND_BODY: &str = "\u{feff}// Я😀\r\n&AtClient\r\nProcedure CommandProcessing(Выбор, Context)\r\n Result=Выбор;\r\nEndProcedure\r\n";

/// A standard request omits optional snapshots, so the server must bind its reply to current inputs itself.
fn parameter(client: &mut Client, uri: &url::Url) -> Value {
    client.ask(&request(json!("parameter"),"textDocument/hover",json!({"textDocument":{"uri":uri.as_str()},"position":position(COMMAND_BODY,COMMAND_BODY.rfind("Выбор").unwrap())})))
}

/// Current type/use, invalid retraction, repair and close preserve native UTF-16 spans and byte hashes RU/EN.
#[test]
fn common_command_parameter_hover_follows_current_yaml_over_stdio_in_both_locales() {
    let (directory, root) = support::project("// Independent module.\n", false);
    crate::command_project::add_commands(
        &root,
        &directory.0.join("native"),
        COMMAND_BODY,
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let source = root.join("src-with-common-commands");
    let path = source.join(COMMAND);
    let saved = fs::read_to_string(&path).unwrap();
    let uri = url::Url::from_file_path(source.join(COMMAND_MODULE)).unwrap();
    let yaml = url::Url::from_file_path(&path).unwrap();
    let mut machines = Vec::new();
    for (lang, string, boolean) in [
        ("en-US", "String", "Boolean"),
        ("ru-RU", "Строка", "Булево"),
    ] {
        let mut client = Client::with_options(&root, lang, &[]);
        let init = client.ask(&initialize_hover(json!(["plaintext"])));
        assert!(init.get("error").is_none());
        client.send(&notification("initialized"));
        let first = parameter(&mut client, &uri);
        assert!(
            first["result"]["contents"]["value"]
                .as_str()
                .unwrap()
                .contains(string),
            "{first}"
        );
        let at = COMMAND_BODY.rfind("Выбор").unwrap();
        assert_eq!(first["result"]["eska"]["kind"], "parameter");
        assert_eq!(first["result"]["eska"]["source"], COMMAND_MODULE);
        assert_eq!(first["result"]["eska"]["source_sha256"], hash(COMMAND_BODY));
        assert_eq!(
            first["result"]["range"],
            json!({"start":position(COMMAND_BODY,at),"end":position(COMMAND_BODY,at+"Выбор".len())})
        );
        let changed = saved.replace("x:string", "x:boolean");
        assert_eq!(client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":yaml.as_str(),"languageId":"eska-source","version":1,"text":changed}}}))["method"],"eska/inputChanged");
        let current = parameter(&mut client, &uri);
        assert!(
            current["result"]["contents"]["value"]
                .as_str()
                .unwrap()
                .contains(boolean),
            "{current}"
        );
        let invalid=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":yaml.as_str(),"version":2},"contentChanges":[{"text":"broken: ["}]}}));
        assert_eq!(invalid["method"], "eska/inputChanged");
        let refused = parameter(&mut client, &uri);
        assert_eq!(
            refused["error"]["data"]["kind"], "analysis_composition_incomplete",
            "{refused}"
        );
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":yaml.as_str(),"version":3},"contentChanges":[{"text":changed}]}}));
        let repaired = parameter(&mut client, &uri);
        assert_eq!(
            repaired["result"]["contents"],
            current["result"]["contents"]
        );
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":yaml.as_str()}}}));
        let closed = parameter(&mut client, &uri);
        assert_eq!(closed["result"]["contents"], first["result"]["contents"]);
        machines.push(
            [first, current, repaired, closed]
                .into_iter()
                .map(machine)
                .collect::<Vec<_>>(),
        );
        client.finish();
    }
    assert_eq!(machines[0], machines[1]);
    assert_eq!(fs::read_to_string(path).unwrap(), saved);
    assert_eq!(
        fs::read_to_string(source.join(COMMAND_MODULE)).unwrap(),
        COMMAND_BODY
    );
}
