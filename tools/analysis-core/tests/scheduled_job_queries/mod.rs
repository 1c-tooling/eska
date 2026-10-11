//! IDE pulls and completions share the current canonical scheduled-job inputs.

#[path = "../support/scheduled_job_project.rs"]
mod fixture;

use super::*;
use std::collections::BTreeSet;

const OWNER: &str = "ScheduledJobs/BeforeRun/object.yaml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "Procedure Run()\n Value = Metadata.ScheduledJobs.BeforeRun;\nEndProcedure\n";

/// URI construction is limited to declared files in this test's own imported source.
fn uri(source: &Path, path: &str) -> String {
    url::Url::from_file_path(source.join(path)).unwrap().into()
}

/// Every pull uses the negotiated semantic provider and last acknowledged revision.
fn diagnostic(source: &Path, path: &str, snap: &Value) -> Value {
    request(
        json!(30),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":uri(source,path)},
        "identifier":"eska-semantic","eska":{"snapshot":snap}}),
    )
}

/// The cursor sits in valid BSL at the actual scheduled-job collection.
fn completion(source: &Path, snap: &Value) -> Value {
    request(
        json!(31),
        "textDocument/completion",
        json!({"textDocument":{"uri":uri(source,MODULE)},
        "position":{"line":1,"character":" Value = Metadata.ScheduledJobs.".len()},"eska":{"snapshot":snap}}),
    )
}

/// Compare actual candidates independently of optional native presentation labels.
fn labels(response: &Value) -> BTreeSet<String> {
    response["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap().into())
        .collect()
}

/// Invalid current flags affect shared coverage and localized metadata findings on both locales.
#[test]
fn scheduled_job_inputs_reach_ide_completions_and_current_metadata_diagnostics() {
    let (directory, root) = support::project(BODY, false);
    fixture::add_jobs(&root, &directory.0.join("native"));
    let source = root.join("src-with-jobs");
    let saved = fs::read_to_string(source.join(OWNER)).unwrap();
    let invalid = saved.replace("<Use>true</Use>", "<Use>Future</Use>");
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &["--profile", "semantic"]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let ready = client.ask(&diagnostic(&source, MODULE, &snapshot("0")));
        assert!(ready.get("error").is_none(), "{ready}");
        assert_eq!(
            ready["result"]["eska"]["coverage"]["semantic_profile_complete"],
            true
        );
        let snap = &ready["result"]["eska"]["snapshot"];
        let names = labels(&client.ask(&completion(&source, snap)));
        assert!(names.contains("BeforeRun") && names.contains("Neighbor"));
        let metadata = client.ask(&request(
            json!(32),
            "eska/metadata/read",
            json!({"source":OWNER,"snapshot":snap}),
        ));
        assert_eq!(metadata["result"]["metadata"]["state"]["model"], "ready");
        let changed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri(&source,OWNER),"languageId":"yaml","version":7,"text":invalid}}}));
        let snap = &changed["params"]["snapshot"];
        assert_eq!(changed["params"]["input"]["state"]["model"], "invalid");
        let partial = client.ask(&diagnostic(&source, MODULE, snap));
        assert_eq!(
            partial["result"]["eska"]["coverage"]["semantic_profile_complete"],
            false
        );
        let metadata = client.ask(&diagnostic(&source, OWNER, snap));
        let item = &metadata["result"]["items"][0];
        assert_eq!(item["code"], "analysis_adapter_invalid");
        assert_eq!(
            item["data"]["issue"],
            json!({"kind":"invalid_property","name":"Use"})
        );
        let mut finding = item.clone();
        finding.as_object_mut().unwrap().remove("message");
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(&source,OWNER)}}}));
        let restored = client.ask(&diagnostic(&source, MODULE, &closed["params"]["snapshot"]));
        assert_eq!(
            restored["result"]["eska"]["coverage"]["semantic_profile_complete"],
            true
        );
        assert_eq!(
            labels(&client.ask(&completion(
                &source,
                &restored["result"]["eska"]["snapshot"]
            ))),
            names
        );
        if let Some(previous) = &previous {
            assert_eq!(&(names.clone(), finding.clone()), previous);
        }
        previous = Some((names, finding));
        client.finish();
    }
    assert_eq!(fs::read_to_string(source.join(OWNER)).unwrap(), saved);
}
