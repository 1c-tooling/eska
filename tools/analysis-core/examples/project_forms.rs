//! Compare current canonical common forms with the pinned native reader on read-only corpora.

use std::{collections::BTreeMap, error::Error, path::PathBuf, time::Instant};

use eska::analysis_core::InputRequest;
use eska_analysis::{AnalysisSession, MetadataResourceKind};
use ide_db::metadata::CanonicalMetadata;

/// Current session resources supply the model; native XML is only an independent reference.
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
        .resource_paths()
        .filter(|path| {
            session
                .metadata_resource(path)
                .is_some_and(|input| input.kind == MetadataResourceKind::FormDialog)
        })
        .map(|path| {
            session
                .resource_owner(path)
                .map(|owner| (owner.clone(), path.clone()))
                .ok_or("missing_form_owner")
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let paths = session
        .owner_paths()
        .filter(|path| path.as_str().starts_with("CommonForms/"))
        .cloned()
        .collect::<Vec<_>>();
    let mut forms = 0;
    let mut elements = 0;
    let mut attributes = 0;
    let mut columns = 0;
    let mut events = 0;
    let mut commands = 0;
    for path in paths {
        let request = InputRequest::default();
        let resource = resources
            .get(&path)
            .ok_or("missing_declared_form_resource")?;
        session
            .load_resource_saved(resource, session.revision(), &request)
            .map_err(|error| format!("{error:?}"))?;
        session
            .load_saved(&path, session.revision(), &request)
            .map_err(|error| format!("{error:?}"))?;
        let Some(CanonicalMetadata::Form(current)) = session
            .resolve(&path, session.revision())
            .map_err(|error| format!("{error:?}"))?
        else {
            return Err(format!(
                "form_unavailable:{}:{:?}",
                path.as_str(),
                session.owner(&path)
            )
            .into());
        };
        let prefix = session
            .native_path(&path)
            .and_then(|path| path.as_str().strip_suffix(".xml"))
            .ok_or("missing_native_form_identity")?;
        let reference = bsl_metadata::xml_parser::parse_form_from_bsl_path(
            &native.join(format!("{prefix}/Ext/Form/Module.bsl")),
        )?;
        if *current != reference {
            return Err(format!("form_model_mismatch:{}", path.as_str()).into());
        }
        forms += 1;
        elements += current.elements.len();
        attributes += current.attributes.len();
        columns += current
            .attributes
            .iter()
            .map(|attribute| attribute.columns.len())
            .sum::<usize>();
        events += current.event_handlers.len();
        commands += current.command_handlers.len();
    }
    if forms == 0 {
        return Err("no_common_forms".into());
    }
    println!(
        "{}",
        serde_json::json!({"common_forms":forms,"form_resources":resources.len(),"elements":elements,"attributes":attributes,"attribute_columns":columns,"event_handlers":events,"command_handlers":commands,"model_parity":true,"main_xml_generated":false,"resources":session.resources(),"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}
