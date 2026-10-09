//! Read-only acceptance probe for the composed metadata session, not a public checker command.

use std::{collections::BTreeMap, error::Error, path::PathBuf, time::Instant};

use eska::analysis_core::InputRequest;
use eska_analysis::{AnalysisSession, ModelState};

/// Load declared saved owners and resolve every supported current model through the real kernel.
fn main() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let [root] = arguments.as_slice() else {
        return Err("expected_eska_source_root".into());
    };
    let started = Instant::now();
    let mut session = AnalysisSession::open(root).map_err(|error| format!("{error:?}"))?;
    let opened = session.summary();
    let mut resolved = 0;
    let mut unsupported = BTreeMap::<String, usize>::new();
    let mut invalid = BTreeMap::<String, usize>::new();
    let paths = session.owner_paths().cloned().collect::<Vec<_>>();
    for path in paths {
        session
            .load_saved(&path, session.revision(), &InputRequest::default())
            .map_err(|error| format!("{error:?}"))?;
        let model = session.owner(&path).ok_or("missing_owner_state")?;
        match &model.state {
            ModelState::Ready(value) => {
                let current = session
                    .resolve(&path, session.revision())
                    .map_err(|error| format!("{error:?}"))?;
                if current.as_ref() != Some(value.as_ref()) {
                    return Err("canonical_resolver_mismatch".into());
                }
                resolved += 1;
            }
            ModelState::Unsupported(kind) => {
                *unsupported.entry(kind.as_str().into()).or_default() += 1;
            }
            ModelState::Invalid { error, .. } => {
                *invalid.entry(format!("{:?}", error.issue)).or_default() += 1;
            }
            _ => {}
        }
    }
    let summary = session.summary();
    println!(
        "{}",
        serde_json::json!({
            "opened_pending": opened.pending,
            "inputs": summary.inputs,
            "resolved": resolved,
            "structure_only": summary.structure_only,
            "unsupported": unsupported,
            "invalid": invalid,
            "unavailable": summary.unavailable,
            "descriptors": summary.descriptors,
            "modeled_descriptors": summary.modeled_descriptors,
            "main_models_complete": summary.main_models_complete,
            "descriptor_models_complete": summary.descriptor_models_complete,
            "xml_exported": false,
            "seconds": started.elapsed().as_secs_f64(),
        })
    );
    if !summary.descriptor_models_complete {
        std::process::exit(2);
    }
    Ok(())
}
