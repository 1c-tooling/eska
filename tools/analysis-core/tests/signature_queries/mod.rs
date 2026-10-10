//! Actual call-signature requests share current source authority and standard client negotiation.

mod controls;
mod cross_module;
mod current;

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::*;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}// Я😀\r\n// Текущая **документация** [пример](command:run).\r\nФункция Вызов(Вызов, Другой = \"😀\", Последний = 0) Экспорт\r\nВозврат Вызов;\r\nКонецФункции\r\nПроцедура Проверка()\r\nЯ=\"😀\";Вызов(Вызов(1), \"x,y\", );\r\nКонецПроцедуры\r\n";

/// Hash the caller's verbatim bytes independently of the server's resource bookkeeping.
fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut hash, byte| {
            write!(hash, "{byte:02x}").unwrap();
            hash
        })
}

/// This independent cursor counts UTF-16, including a non-BMP character on the call's line.
fn position(text: &str, at: usize) -> Value {
    let before = &text[..at];
    json!({"line":before.bytes().filter(|byte| *byte == b'\n').count(),
        "character":before.rsplit('\n').next().unwrap().encode_utf16().count()})
}

/// The optional snapshot guards standard signature-help params and current call-site positions.
fn pull(root: &Path, module: &str, source: &str, revision: &str, position: Value) -> Value {
    let mut params = json!({
        "textDocument":{"uri":url::Url::from_file_path(root.join(source).join(module)).unwrap().as_str()},
        "eska":{"snapshot":snapshot(revision)},"workDoneToken":"accepted"});
    params["position"] = position;
    request(json!("параметры"), "textDocument/signatureHelp", params)
}

/// A capable client explicitly negotiates parameter-label offsets and modern active selection.
fn initialize_signature(format: &str, offsets: bool) -> Value {
    let mut value = initialize();
    value["params"]["capabilities"] = json!({"textDocument":{"signatureHelp":{
        "contextSupport":true,"signatureInformation":{"documentationFormat":["unknown",format],
            "parameterInformation":{"labelOffsetSupport":offsets},
            "activeParameterSupport":true,"noActiveParameterSupport":true}}}});
    value
}

/// Verify current caller bytes and independently decode generated UTF-16 parameter ranges.
fn verify(result: &Value, text: &str, module: &str, revision: &str, at: usize, offsets: bool) {
    assert!(result.get("signatures").is_some(), "{result}");
    assert_eq!(result["eska"]["snapshot"], snapshot(revision));
    assert_eq!(result["eska"]["source"], module);
    assert_eq!(result["eska"]["source_sha256"], hash(text));
    assert_eq!(result["eska"]["profile"], "call-signature");
    assert_eq!(result["eska"]["position_byte"], at);
    assert_eq!(result["eska"]["coverage"]["semantic_complete"], false);
    for signature in result["signatures"].as_array().unwrap() {
        let label = signature["label"].as_str().unwrap();
        let units = label.encode_utf16().collect::<Vec<_>>();
        let mut previous = label.find('(').unwrap() + 1;
        for parameter in signature["parameters"].as_array().unwrap() {
            let displayed = if offsets {
                let start = usize::try_from(parameter["label"][0].as_u64().unwrap()).unwrap();
                let end = usize::try_from(parameter["label"][1].as_u64().unwrap()).unwrap();
                String::from_utf16(&units[start..end]).unwrap()
            } else {
                parameter["label"].as_str().unwrap().to_owned()
            };
            assert!(label[previous..].starts_with(&displayed), "{signature}");
            previous += displayed.len() + 2;
        }
    }
}
