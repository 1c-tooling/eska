//! Transport fixtures verify policy and safety boundaries; actual formatter/native acceptance runs separately.

use super::*;
use crate::ide::server::{Identity, Server};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

use crate::test_support::TestDir;
use eska_project_core::formatting::protocol::source_sha256;

struct Fixture {
    directory: TestDir,
    server: Server,
    binary: PathBuf,
}

impl Fixture {
    /// One executable fixture records the supplied policy and can deliberately return unsafe byte plans.
    fn new(mode: &str) -> Self {
        let directory = TestDir::new();
        let binary = directory.0.join("formatter.py");
        let contract = json!({"schema_version":contract::SCHEMA_VERSION,"tool":"eska-fmt","target":contract::TARGET,"version":"0.18.0","config_schema":contract::CONFIG_SCHEMA,"bridge_schema":contract::BRIDGE_SCHEMA,"metadata_api":null});
        let mut script = format!(
            "#!/usr/bin/env python3\nimport sys,json,hashlib\nfrom pathlib import Path\ncontract=json.loads({})\n",
            serde_json::to_string(&contract.to_string()).unwrap()
        );
        script.push_str(r#"if sys.argv[1] == '__tool-contract':
    print(json.dumps(contract))
    sys.exit(0)
root=Path(__file__).parent
request=json.load(sys.stdin)
(root/'input.json').write_text(json.dumps(request))
mode=(root/'mode').read_text()
text=request['text']
response={'schema_version':1,'source_sha256':hashlib.sha256(text.encode()).hexdigest(),'edits':[], 'formatted_range':{'start':0,'end':len(text.encode())}}
if mode=='replace': response['edits']=[{'range':{'start':0,'end':len(text.encode())},'text':'A = 1;\n'}]
if mode=='bad_hash': response['source_sha256']='0'*64
if mode=='bad_range': response['edits']=[{'range':{'start':1,'end':2},'text':'X'}]
if mode=='source_change': (root/'Module.bsl').write_text('external edit')
if mode=='manifest_change': (root/'eska.toml').write_text("[project]\ntype='configuration'\n")
print(json.dumps(response))
"#);
        fs::write(&binary, script).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(directory.0.join("mode"), mode).unwrap();
        fs::write(directory.0.join("Module.bsl"), "A=1;").unwrap();
        let mut server = Server::new(Identity::Project).unwrap();
        server.source_root = Some(directory.0.clone());
        initialize(&mut server, 33);
        Self {
            directory,
            server,
            binary,
        }
    }

    /// The formatting API needs no metadata membership, analyzer executable or write to its source.
    fn document(&mut self, text: &str) -> Result<Value, Value> {
        let input = json!({"source":{"encoding":"utf-8","value":self.directory.0.join("Module.bsl")},"version":"7","text":text,"userReadOnly":false,"formatter":{"encoding":"utf-8","value":self.binary}});
        self.call("format/document", &input)
    }

    /// Direct server dispatch preserves the same lifecycle/API gate as a framed request.
    fn call(&mut self, method: &str, input: &Value) -> Result<Value, Value> {
        self.server.execute(
            method,
            input,
            &mut Vec::new(),
            &AtomicBool::new(false),
            &AtomicBool::new(false),
        )
    }
}

/// Each API minor receives only capabilities it can safely invoke.
fn initialize(server: &mut Server, minor: u32) -> Value {
    server.execute("initialize",&json!({"apiVersion":{"major":1,"minor":minor},"client":{"name":"fmt-test","version":"1"},"locale":"ru-RU"}),&mut Vec::new(),&AtomicBool::new(false),&AtomicBool::new(false)).unwrap()
}

/// Pure byte plans and effective policy cross the component boundary without publishing the source.
#[test]
fn document_formatting_delegates_policy_and_keeps_the_file_unchanged() {
    let mut fixture = Fixture::new("replace");
    let result = fixture.document("A=1;").unwrap();
    assert_eq!(result["version"], "7");
    assert_eq!(result["formatterVersion"], "0.18.0");
    assert_eq!(result["plan"]["edits"][0]["text"], "A = 1;\n");
    assert_eq!(
        fs::read_to_string(fixture.directory.0.join("Module.bsl")).unwrap(),
        "A=1;"
    );
    let input: Value =
        serde_json::from_slice(&fs::read(fixture.directory.0.join("input.json")).unwrap()).unwrap();
    assert_eq!(input["language"], "bsl");
    assert_eq!(input["options"]["indent_style"], "tab");
    assert_eq!(input["options"]["indent_width"], 4);
}

/// A future/malformed formatter cannot cause UTF-8 corruption or apply a plan to a different snapshot.
#[test]
fn unsafe_component_replies_are_rejected() {
    for (mode, text, field) in [
        ("bad_hash", "A=1;", "snapshot"),
        ("bad_range", "Я=1;", "edits"),
    ] {
        let mut fixture = Fixture::new(mode);
        let result = fixture.document(text).unwrap_err();
        assert_eq!(result["data"]["details"]["field"], field);
        assert_eq!(
            fs::read_to_string(fixture.directory.0.join("Module.bsl")).unwrap(),
            "A=1;"
        );
    }
}

/// External disk/configuration changes during child execution invalidate even an otherwise safe response.
#[test]
fn intervening_source_and_style_changes_reject_the_plan() {
    for (mode, code) in [
        ("source_change", "source_conflict"),
        ("manifest_change", "config_conflict"),
    ] {
        let mut fixture = Fixture::new(mode);
        let result = fixture.document("A=1;").unwrap_err();
        assert_eq!(result["data"]["details"]["code"], code);
    }
}

/// Read-only native state is rejected before invoking the optional formatter.
#[test]
fn read_only_document_does_not_start_a_component() {
    let mut fixture = Fixture::new("replace");
    let result = fixture
        .call(
            "format/document",
            &json!({"version":"7","text":"A=1;","userReadOnly":true}),
        )
        .unwrap_err();
    assert_eq!(result["data"]["details"]["code"], "source_read_only");
    assert!(!fixture.directory.0.join("input.json").exists());
}

/// An excluded source is a no-op before decoding disk bytes or invoking an optional executable.
#[test]
fn excluded_document_does_not_read_or_invoke_the_formatter() {
    let mut fixture = Fixture::new("replace");
    fs::create_dir(fixture.directory.0.join("src")).unwrap();
    fs::write(
        fixture.directory.0.join("eska.toml"),
        "[project]\ntype='configuration'\n[format]\nexclude=['Module.bsl']\n",
    )
    .unwrap();
    fs::write(fixture.directory.0.join("Module.bsl"), [0xff, 0xfe]).unwrap();
    let result = fixture.document("Native text").unwrap();
    assert_eq!(result["plan"]["edits"], json!([]));
    assert!(result["formatterVersion"].is_null());
    assert!(!fixture.directory.0.join("input.json").exists());
}

/// Older clients cannot allocate new formatting buffers or accidentally get a second formatter path.
#[test]
fn capability_and_methods_are_gated_by_api_minor() {
    let mut server = Server::new(Identity::Project).unwrap();
    let result = initialize(&mut server, 32);
    assert!(result["capabilities"].get("formatting").is_none());
    assert_eq!(
        server
            .execute(
                "format/bufferBegin",
                &json!({"bytes":0,"snapshot":source_sha256("")}),
                &mut Vec::new(),
                &AtomicBool::new(false),
                &AtomicBool::new(false)
            )
            .unwrap_err()["code"],
        -32601
    );
    let mut server = Server::new(Identity::Project).unwrap();
    let result = initialize(&mut server, 33);
    assert_eq!(
        result["capabilities"]["formatting"]["component"],
        "eska-fmt"
    );
}

/// Complete uploaded text uses existing bounded storage and is released independently of a metadata workspace.
#[test]
fn large_document_upload_is_checked_and_released() {
    let mut fixture = Fixture::new("none");
    let rejected = fixture
        .call(
            "format/bufferBegin",
            &json!({"bytes":4*1024*1024+1,"snapshot":source_sha256("")}),
        )
        .unwrap_err();
    assert_eq!(rejected["data"]["details"]["code"], "text_too_large");
    let text = "A=1;\n".repeat(24_000);
    let opened = fixture
        .call(
            "format/bufferBegin",
            &json!({"bytes":text.len(),"snapshot":source_sha256(&text)}),
        )
        .unwrap();
    let id = opened["bufferId"].as_str().unwrap();
    for (index, chunk) in text.as_bytes().chunks(64 * 1024).enumerate() {
        fixture.call("format/bufferAppend",&json!({"bufferId":id,"offset":index*64*1024,"text":std::str::from_utf8(chunk).unwrap()})).unwrap();
    }
    let result=fixture.call("format/document",&json!({"source":{"encoding":"utf-8","value":fixture.directory.0.join("Module.bsl")},"version":"9","bufferId":id,"userReadOnly":false,"formatter":{"encoding":"utf-8","value":fixture.binary}})).unwrap();
    assert_eq!(result["plan"]["source_sha256"], source_sha256(&text));
    assert_eq!(
        fixture
            .call("format/bufferClose", &json!({"bufferId":id}))
            .unwrap()["released"],
        true
    );
}
