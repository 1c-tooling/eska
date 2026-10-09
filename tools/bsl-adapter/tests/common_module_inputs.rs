//! Saved and unsaved YAML enter the same converter through the real project input controller.

mod support;

use std::{fs, sync::Arc};

use eska::{
    analysis_core::{InputRequest, InputState, MetadataInputs},
    project_core::source_format::{MetadataSnapshot, SourcePath, import_designer},
};
use eska_bsl_adapter::{AdapterIssue, common_module};
use support::{TestDirectory, native_corpus};

/// Only the current ready snapshot may feed the analyzer; a prior value is never substituted.
fn ready(inputs: &MetadataInputs, path: &SourcePath) -> Arc<MetadataSnapshot> {
    let InputState::Ready(value) = &inputs.owner(path).unwrap().state else {
        panic!("expected ready snapshot");
    };
    Arc::clone(value)
}

/// Exercise equality, context changes, exact error locations and restoration without XML export.
#[test]
fn module_drafts_share_saved_semantics_and_invalid_inputs_have_no_stale_model() {
    let directory = TestDirectory::new();
    let source = directory.0.join("source");
    let corpus = native_corpus();
    import_designer(&corpus, &source).unwrap();
    let mut inputs = MetadataInputs::open(&source).unwrap();
    let path = inputs
        .owner_paths()
        .find(|path| path.as_str().starts_with("CommonModules/Tools/"))
        .unwrap()
        .clone();
    let request = InputRequest::default();
    inputs
        .load_saved(&path, inputs.revision(), &request)
        .unwrap();
    let saved = ready(&inputs, &path);
    let saved_model = common_module(&saved.documents[0].descriptor.objects[0]).unwrap();
    let native = fs::read_to_string(corpus.join("CommonModules/Tools.xml")).unwrap();
    assert_eq!(
        saved_model,
        bsl_metadata::xml_parser::parse_common_module_xml(&native).unwrap()
    );
    let text = fs::read_to_string(source.join(path.as_path())).unwrap();
    inputs
        .apply_overlay(&path, inputs.revision(), 1, &text, &request)
        .unwrap();
    let same = ready(&inputs, &path);
    assert_eq!(same, saved);
    assert_eq!(
        common_module(&same.documents[0].descriptor.objects[0]).unwrap(),
        saved_model
    );

    assert_eq!(text.matches("<Server>true</Server>").count(), 1);
    let draft = text.replace("<Server>true</Server>", "<Server>false</Server>");
    inputs
        .apply_overlay(&path, inputs.revision(), 2, &draft, &request)
        .unwrap();
    let changed = ready(&inputs, &path);
    let model = common_module(&changed.documents[0].descriptor.objects[0]).unwrap();
    assert!(!model.is_server());
    assert!(saved_model.is_server());
    assert_ne!(changed.source_sha256, saved.source_sha256);

    let invalid = text.replace("<Server>true</Server>", "<Server>TRUE</Server>");
    inputs
        .apply_overlay(&path, inputs.revision(), 3, &invalid, &request)
        .unwrap();
    let parsed = ready(&inputs, &path);
    let error = common_module(&parsed.documents[0].descriptor.objects[0]).unwrap_err();
    assert_eq!(
        error.issue,
        AdapterIssue::InvalidProperty {
            name: "Server".into()
        }
    );
    assert!(invalid[error.range].contains("<Server>TRUE</Server>"));

    inputs
        .apply_overlay(&path, inputs.revision(), 4, "document: [", &request)
        .unwrap();
    assert!(matches!(
        inputs.owner(&path).unwrap().state,
        InputState::Unavailable(_)
    ));
    inputs
        .close_overlay(&path, inputs.revision(), &request)
        .unwrap();
    assert_eq!(ready(&inputs, &path), saved);
    assert_eq!(
        fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
    assert!(!source.join("Configuration.xml").exists());
    assert!(!source.join("CommonModules/Tools.xml").exists());
}
