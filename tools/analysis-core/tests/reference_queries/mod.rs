//! Standard reference locations retain current caller/target hashes, scope and search boundaries.

mod controls;
mod cross_module;
mod current;

use super::*;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}Функция Значение(Аргумент) Экспорт\r\nЯ=\"😀\";Возврат Аргумент;\r\nКонецФункции\r\nФункция Другой(Аргумент)\r\nВозврат Аргумент;\r\nКонецФункции\r\nРезультат=Значение(1);\r\n";

/// Hash caller and target bytes independently rather than trusting the response's bookkeeping.
fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut hash, byte| {
            write!(hash, "{byte:02x}").unwrap();
            hash
        })
}

/// An independent UTF-16 cursor includes the non-BMP character preceding a parameter read.
fn position(text: &str, at: usize) -> Value {
    let before = &text[..at];
    json!({"line":before.bytes().filter(|byte| *byte==b'\n').count(),
        "character":before.rsplit('\n').next().unwrap().encode_utf16().count()})
}

/// Standard reference context is mandatory; both progress tokens are accepted without streaming.
fn pull(
    root: &Path,
    module: &str,
    source: &str,
    revision: &str,
    position: Value,
    declaration: bool,
) -> Value {
    let mut params = json!({"textDocument":{"uri":url::Url::from_file_path(root.join(source).join(module)).unwrap().as_str()},
        "context":{"includeDeclaration":declaration},"eska":{"snapshot":snapshot(revision)},
        "workDoneToken":"accepted","partialResultToken":1});
    params["position"] = position;
    request(json!("использования"), "textDocument/references", params)
}

/// Every occurrence certifies current target text, independently of its caller's source bytes.
fn verify(
    result: &Value,
    caller: &str,
    caller_text: &str,
    target_text: &str,
    revision: &str,
    name: &str,
) {
    assert!(result.get("uri").is_some(), "{result}");
    let ctx = &result["eska"];
    let target = &ctx["target"];
    let start = usize::try_from(target["start_byte"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(target["end_byte"].as_u64().unwrap()).unwrap();
    assert_eq!(ctx["snapshot"], snapshot(revision));
    assert_eq!(ctx["source"], caller);
    assert_eq!(ctx["source_sha256"], hash(caller_text));
    assert_eq!(ctx["profile"], "references");
    assert_eq!(ctx["coverage"]["semantic_complete"], false);
    assert_eq!(target["source_sha256"], hash(target_text));
    assert_eq!(&target_text[start..end], name);
    assert_eq!(
        result["range"],
        json!({"start":position(target_text,start),"end":position(target_text,end)})
    );
    assert_eq!(ctx["search"]["files_capped"], false);
    assert_eq!(ctx["search"]["hits_capped"], false);
}
