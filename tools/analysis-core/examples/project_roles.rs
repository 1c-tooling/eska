//! Compare current role resolvers with the pinned native reader on read-only corpora.

use std::{collections::BTreeMap, error::Error, fs, path::PathBuf, time::Instant};

use eska::analysis_core::InputRequest;
use eska_analysis::AnalysisSession;
use ide_db::metadata::CanonicalMetadata;

/// Use explicit session inputs; native XML is read only for independent model comparison.
fn main() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let [source, native] = arguments.as_slice() else {
        return Err("expected_source_and_native_roots".into());
    };
    let started = Instant::now();
    let mut session = AnalysisSession::open(source).map_err(|error| format!("{error:?}"))?;
    let resources = session
        .role_rights_paths()
        .map(|path| {
            session
                .role_rights_owner(path)
                .map(|owner| (owner.clone(), path.clone()))
                .ok_or("missing_role_owner")
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let paths = session
        .owner_paths()
        .filter(|path| path.as_str().starts_with("Roles/"))
        .cloned()
        .collect::<Vec<_>>();
    let mut roles = 0;
    let mut references = 0;
    let mut restrictions = 0;
    for path in paths {
        let request = InputRequest::default();
        if let Some(resource) = resources.get(&path) {
            session
                .load_role_rights_saved(resource, session.revision(), &request)
                .map_err(|error| format!("{error:?}"))?;
        }
        session
            .load_saved(&path, session.revision(), &request)
            .map_err(|error| format!("{error:?}"))?;
        let Some(CanonicalMetadata::Role(current)) = session
            .resolve(&path, session.revision())
            .map_err(|error| format!("{error:?}"))?
        else {
            return Err(format!(
                "role_unavailable:{}:{:?}",
                path.as_str(),
                session.owner(&path)
            )
            .into());
        };
        let native_path = session
            .native_path(&path)
            .ok_or("missing_native_role_identity")?;
        let main_xml = fs::read_to_string(native.join(native_path.as_path()))?;
        let native_rights = resources
            .get(&path)
            .map(|resource| fs::read_to_string(native.join(resource.as_path())))
            .transpose()?;
        // Refuse an invalid reference rather than compare against the legacy reader's default fallback.
        if let Some(text) = &native_rights {
            bsl_metadata::xml_parser::parse_rights_xml(text)?;
        }
        let reference = bsl_metadata::parse_role_from_texts(&main_xml, native_rights.as_deref())
            .ok_or("invalid_native_role")?;
        if *current != reference {
            return Err(format!("role_model_mismatch:{}", path.as_str()).into());
        }
        roles += 1;
        references += current.objects().len();
        restrictions += current
            .objects()
            .iter()
            .map(|object| object.restrictions.len())
            .sum::<usize>();
    }
    if roles == 0 {
        return Err("no_roles".into());
    }
    println!(
        "{}",
        serde_json::json!({"roles":roles,"rights_resources":resources.len(),"object_references":references,"rls_conditions":restrictions,"model_parity":true,"main_xml_generated":false,"resources":session.resources(),"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}
