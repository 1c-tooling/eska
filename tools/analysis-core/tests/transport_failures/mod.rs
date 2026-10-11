//! Framed invalid JSON can recover; damaged frame boundaries must terminate the actual server.

use super::*;

/// Rejected JSON never consumes a request ID, changes the revision, or poisons the next valid frame.
#[test]
fn malformed_json_is_recoverable_in_both_locales() {
    let (_directory, root) = support::project("// Current module.\n", false);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        assert!(client.ask(&initialize()).get("result").is_some());
        client.send(&notification("initialized"));
        for (body, code, kind) in [
            (b"{".as_slice(), -32700, "rpc_parse_error"),
            (b"[]".as_slice(), -32600, "rpc_invalid_request"),
            (
                br#"{"jsonrpc":"2.0","id":2,"id":3,"method":"shutdown"}"#.as_slice(),
                -32600,
                "rpc_invalid_request",
            ),
        ] {
            rpc_framing::write_frame(&mut client.input, body).unwrap();
            let rejected = client.receive();
            assert_eq!(rejected["jsonrpc"], "2.0");
            assert!(rejected["id"].is_null());
            assert_eq!(rejected["error"]["code"], code);
            assert_eq!(rejected["error"]["data"]["kind"], kind);
            assert!(rejected.get("result").is_none());
            let next = client.ask(&request(
                json!(2),
                "eska/metadata/owners",
                json!({"snapshot":snapshot("0"),"limit":200}),
            ));
            assert_eq!(next["id"], 2);
            assert_eq!(next["result"]["snapshot"], snapshot("0"));
            assert!(next["result"]["owners"].is_array());
        }
        client.finish();
    }
}

/// An invalid header or truncated frame closes the process with no partial JSON-RPC response.
#[test]
fn broken_framing_terminates_without_resynchronization() {
    let directory = support::TestDirectory::new();
    for lang in ["en-US", "ru-RU"] {
        for wire in [
            b"Content-Length: nope\r\n\r\n".as_slice(),
            b"Content-Length: 9\r\n\r\n{}".as_slice(),
        ] {
            let mut child = Command::new(env!("CARGO_BIN_EXE_eska-analyzer"))
                .current_dir(&directory.0)
                .env("PATH", "")
                .args(["--lang", lang])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            input.write_all(wire).unwrap();
            drop(input);
            let output = child.wait_with_output().unwrap();
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        }
    }
}

/// Shutdown retires the session before further requests but still permits the final exit notification.
#[test]
fn shutdown_rejects_further_requests_in_both_locales() {
    let (_directory, root) = support::project("// Current module.\n", false);
    for lang in ["en-US", "ru-RU"] {
        let (output, responses) = session(
            &root,
            lang,
            &[],
            &[
                initialize(),
                notification("initialized"),
                ending()[0].clone(),
                request(json!("после shutdown"), "eska/metadata/owners", json!({})),
                ending()[1].clone(),
            ],
        );
        assert!(output.status.success());
        assert!(responses[0].get("result").is_some());
        assert_eq!(responses[1].get("result"), Some(&Value::Null));
        assert_eq!(responses[2]["id"], "после shutdown");
        assert_eq!(responses[2]["error"]["code"], -32600);
        assert_eq!(responses[2]["error"]["data"]["kind"], "server_shutdown");
        assert!(responses[2].get("result").is_none());
    }
}
