//! Current symbol hovers exercise actual stdio, markup, localization and shared input controls.

mod controls;
mod cross_module;
mod current;
mod dependencies;

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::*;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}// Я😀\r\n// Текущая **документация** [пример](command:run).\r\nФункция Значение() Экспорт\r\nВозврат 1;\r\nКонецФункции\r\nЯ=\"😀\";Результат=Значение();\r\n";

/// Hash verbatim bytes independently of the server's input accounting.
fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut hash, byte| {
            write!(hash, "{byte:02x}").unwrap();
            hash
        })
}

/// The independent cursor is measured in UTF-16, including a non-BMP character on the same line.
fn position(text: &str, at: usize) -> Value {
    let before = &text[..at];
    json!({"line":before.bytes().filter(|byte| *byte == b'\n').count(),
        "character":before.rsplit('\n').next().unwrap().encode_utf16().count()})
}

/// Each pull carries an optional current snapshot and only standard hover progress parameters.
fn pull(root: &Path, module: &str, source: &str, revision: &str, position: Value) -> Value {
    let mut params = json!({
        "textDocument":{"uri":url::Url::from_file_path(root.join(source).join(module)).unwrap().as_str()},
        "eska":{"snapshot":snapshot(revision)},"workDoneToken":"accepted"});
    params["position"] = position;
    request(json!("подсказка"), "textDocument/hover", params)
}

/// Negotiate one standard format and explicitly retain client preference order.
fn initialize_hover(formats: Value) -> Value {
    let mut value = initialize();
    value["params"]["capabilities"] = json!({"textDocument":{"hover":{}}});
    value["params"]["capabilities"]["textDocument"]["hover"]["contentFormat"] = formats;
    value
}

/// Check ranges, hashes and source accounting against current fixture bytes, not saved offsets.
fn verify(result: &Value, text: &str, module: &str, revision: &str, at: usize, name: &str) {
    assert_eq!(result["eska"]["snapshot"], snapshot(revision));
    assert_eq!(result["eska"]["source"], module);
    assert_eq!(result["eska"]["source_sha256"], hash(text));
    assert_eq!(result["eska"]["kind"], "method");
    assert_eq!(result["eska"]["profile"], "symbol");
    assert_eq!(result["eska"]["start_byte"], at);
    assert_eq!(result["eska"]["end_byte"], at + name.len());
    assert_eq!(
        result["range"],
        json!({"start":position(text,at),"end":position(text,at+name.len())})
    );
    assert_eq!(result["eska"]["coverage"]["semantic_complete"], false);
}

/// Only the declared human field may differ between locales; every machine field is retained.
fn machine(mut reply: Value) -> Value {
    if let Some(contents) = reply
        .pointer_mut("/result/contents")
        .and_then(Value::as_object_mut)
    {
        contents.remove("value");
    }
    reply
}
