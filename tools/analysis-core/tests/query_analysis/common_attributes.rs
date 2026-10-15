//! Native standalone query diagnostics follow current common membership through the real framed writer.

use super::*;
use eska::project_core::source_format::SourcePath;

/// Count only the native missing-field rule without conflating independent quality diagnostics with metadata readiness.
fn missing(reply: &Value) -> usize {
    assert!(reply.get("error").is_none(), "{reply}");
    reply["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["code"] == "UnknownFieldInQuery")
        .count()
}

/// CF and selected CFE expose the same current field; malformed controls refuse rather than resurrecting a saved type.
#[test]
fn common_attributes_cf_cfe_current_membership_and_invalid_repair_close_are_bilingual() {
    for composition in [false, true] {
        let directory = TestDirectory::new();
        let project = if composition {
            fixture::composition_with_common_attribute(&directory.0).0
        } else {
            fixture::project_with_common_attribute(&directory.0)
        };
        let common = SourcePath::parse("CommonAttributes/SharedЯ/object.yaml").unwrap();
        let saved = fs::read_to_string(project.root.join("src").join(common.as_path())).unwrap();
        let before = fixture::snapshot(&project);
        let mut variants = Vec::new();
        for lang in ["ru-RU", "en-US"] {
            let (mut client, _) = start(&project, lang, true, true, &[]);
            let query_uri = uri(&project, &project.source);
            open(
                &mut client,
                &query_uri,
                1,
                "\u{feff}// Я😀\r\nSELECT Data.SharedЯ AS Value FROM Catalog.Items AS Data",
            );
            assert_eq!(
                missing(&pull(&mut client, &query_uri, Some("eska-query-analysis"))),
                0
            );
            let common_uri = uri(&project, &common);
            let disabled = saved.replace("<r:Use>Use</r:Use>", "<r:Use>DontUse</r:Use>");
            let ack = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":common_uri,"languageId":"eska-source","version":7,"text":disabled}}}));
            assert_eq!(ack["method"], "eska/inputChanged");
            let current = pull(&mut client, &query_uri, Some("eska-query-analysis"));
            assert_eq!(missing(&current), 1);
            variants.push(machine(&current));
            let invalid = saved.replace("<AutoUse>DontUse</AutoUse>", "<AutoUse>Use</AutoUse>");
            let ack = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":common_uri,"version":8},"contentChanges":[{"text":invalid}]}}));
            assert_eq!(ack["method"], "eska/inputChanged");
            let refused = pull(&mut client, &query_uri, Some("eska-query-analysis"));
            assert_eq!(
                refused["error"]["data"]["kind"], "analysis_composition_incomplete",
                "{refused}"
            );
            let repaired = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":common_uri,"version":9},"contentChanges":[{"text":saved.replace("x:string", "x:boolean")}]}}));
            assert_eq!(repaired["method"], "eska/inputChanged");
            assert_eq!(
                missing(&pull(&mut client, &query_uri, Some("eska-query-analysis"))),
                0
            );
            assert_eq!(client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":common_uri}}}))["method"], "eska/inputChanged");
            assert_eq!(
                missing(&pull(&mut client, &query_uri, Some("eska-query-analysis"))),
                0
            );
            client.finish();
        }
        assert_eq!(variants[0], variants[1]);
        assert_eq!(fixture::snapshot(&project), before);
    }
}
