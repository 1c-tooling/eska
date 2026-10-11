//! Serialization limits count JSON-escaped UTF-8 bytes, independently of the framed transport.

use super::*;

/// Exact caps accept the complete wire body, while one byte less rejects it without exceeding the cap.
#[test]
fn serialization_limit_counts_encoded_bytes() {
    let response = json!({"jsonrpc":"2.0","id":"запрос😀","result":"\"строка\"\n\\"});
    let expected = serde_json::to_vec(&response).unwrap();
    let mut body = Vec::new();
    serialize(&response, &mut body, expected.len()).unwrap();
    assert_eq!(body, expected);
    for limit in [0, 1, expected.len() - 1] {
        assert!(serialize(&response, &mut body, limit).is_err());
        assert!(body.len() <= limit);
    }
}

/// A normal result/error/notification retains its complete envelope and fits at its exact size.
#[test]
fn small_envelopes_are_not_rewritten() {
    for response in [
        json!({"jsonrpc":"2.0","id":1,"result":{"value":"😀"}}),
        json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
        json!({"jsonrpc":"2.0","method":"eska/inputsChanged","params":{"revision":"2"}}),
    ] {
        let expected = serde_json::to_vec(&response).unwrap();
        assert_eq!(encode(&response, None, expected.len()).unwrap(), expected);
    }
}

/// Overflow retains correlation and snapshot even before a session exists; no partial result escapes.
#[test]
fn oversized_responses_preserve_identity_and_current_snapshot() {
    const LIMIT: usize = 512;
    for id in [json!(1), json!("запрос😀"), Value::Null] {
        for snapshot in [None, Some(json!({"session_id":"session-1","revision":"7"}))] {
            let response = json!({"jsonrpc":"2.0","id":id,"result":"x".repeat(LIMIT)});
            let body = encode(&response, snapshot.as_ref(), LIMIT).unwrap();
            assert!(body.len() <= LIMIT);
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap(),
                json!({"jsonrpc":"2.0","id":id,"error":{
                    "code":-32000,"message":"Request failed","data":{
                        "kind":"response_too_large","details":{"limit":LIMIT,"snapshot":snapshot}
                    }
                }})
            );
        }
    }
}

/// The caller must terminate without a frame when even the correlated error cannot fit.
#[test]
fn oversized_fallback_returns_an_error_instead_of_partial_json() {
    let response = json!({"jsonrpc":"2.0","id":"x".repeat(512),"result":null});
    assert!(encode(&response, None, 512).is_err());
    assert!(encode(&response, None, 0).is_err());
}

/// Late cancellation replaces serialized success bytes without leaking a suffix or losing its request ID.
#[test]
fn replacing_response_discards_previous_bytes() {
    let response = json!({"jsonrpc":"2.0","id":8,"result":"x".repeat(400)});
    let mut body = encode(&response, None, 512).unwrap();
    let cancelled =
        json!({"jsonrpc":"2.0","id":8,"error":{"code":-32800,"message":"Request cancelled"}});
    serialize(&cancelled, &mut body, 512).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), cancelled);
}
