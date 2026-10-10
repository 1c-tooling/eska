//! Actual stdio rename plans bind native local scope to an open version and pinned module policy.

mod cancellation;
mod controls;
mod current;
mod policy;

use super::*;
use eska::project_core::source_format::{AcceptedRef, NativeUuid, accept_source};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}Функция Значение(Аргумент) Экспорт\r\nПерем Локальная;\r\nЯ=\"😀\";Локальная=Аргумент;\r\nНеявная=Локальная;\r\nВозврат Неявная;\r\nКонецФункции\r\nФункция Другой(Аргумент)\r\nВозврат Аргумент;\r\nКонецФункции\r\n";

/// Each accepted base belongs to one tiny test project; no existing demo supplies trust or policy.
fn project(body: &str, frozen: bool) -> (support::TestDirectory, std::path::PathBuf, AcceptedRef) {
    let (directory, root) = support::project(body, false);
    fs::create_dir(root.join(".eska")).unwrap();
    let freeze = if frozen {
        vec![NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap()]
    } else {
        Vec::new()
    };
    let reference =
        accept_source(&root.join("src"), &root.join(".eska/accepted"), &freeze).unwrap();
    (directory, root, reference)
}

/// Versioned workspace edits are required; prepare support separately controls provider shape.
fn initialize_rename(prepare: bool) -> Value {
    let mut value = initialize();
    value["params"]["capabilities"] = json!({"workspace":{"workspaceEdit":{"documentChanges":true}},
        "textDocument":{"rename":{"prepareSupport":prepare}}});
    value
}

/// Normal notifications retain BOM/CRLF/non-BMP exactly in the actual server's document store.
fn open(client: &mut Client, root: &Path, text: &str, version: i32) -> Value {
    client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":url::Url::from_file_path(root.join("src").join(MODULE)).unwrap().as_str(),
            "languageId":"bsl","version":version,"text":text}}}),
    )
}

/// Independent LF/CRLF coordinates include both BOM and surrogate pairs before each target.
fn position(text: &str, offset: usize) -> Value {
    let prefix = &text[..offset];
    json!({"line":prefix.bytes().filter(|byte|*byte==b'\n').count(),
        "character":prefix.rsplit('\n').next().unwrap().encode_utf16().count()})
}

/// Standard rename parameters never carry a custom document text or unversioned edit preference.
fn query(root: &Path, text: &str, at: usize, revision: &str, replacement: Option<&str>) -> Value {
    let mut params = json!({"textDocument":{"uri":url::Url::from_file_path(root.join("src").join(MODULE)).unwrap().as_str()},
        "position":position(text,at),"workDoneToken":1,"eska":{"snapshot":snapshot(revision)}});
    if let Some(name) = replacement {
        params["newName"] = json!(name);
    }
    request(
        json!("переименовать"),
        if replacement.is_some() {
            "textDocument/rename"
        } else {
            "textDocument/prepareRename"
        },
        params,
    )
}

/// Verify source bytes independently of the server's stored fingerprint.
fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut hash, byte| {
            write!(hash, "{byte:02x}").unwrap();
            hash
        })
}

/// Apply the returned plan only in owned test memory using an independent UTF-16 inverse.
fn apply(text: &str, plan: &Value) -> String {
    let mut result = text.to_owned();
    let offset = |position: &Value| {
        let line = usize::try_from(position["line"].as_u64().unwrap()).unwrap();
        let character = usize::try_from(position["character"].as_u64().unwrap()).unwrap();
        let start = text
            .split_inclusive('\n')
            .take(line)
            .map(str::len)
            .sum::<usize>();
        let prefix = text[start..]
            .split('\n')
            .next()
            .unwrap()
            .encode_utf16()
            .take(character)
            .collect::<Vec<_>>();
        start + String::from_utf16(&prefix).unwrap().len()
    };
    for edit in plan["documentChanges"][0]["edits"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
    {
        result.replace_range(
            offset(&edit["range"]["start"])..offset(&edit["range"]["end"]),
            edit["newText"].as_str().unwrap(),
        );
    }
    result
}
