//! Compare resident nested-form models with the native reader, releasing each owner's source inputs.

use std::{collections::BTreeMap, error::Error, path::PathBuf, time::Instant};

use eska::analysis_core::InputRequest;
use eska_analysis::{AnalysisSession, ModelState};
use ide_db::metadata::CanonicalMetadata;

/// One owner's YAML and dialogs produce independent slots without retaining all corpus source bytes.
fn main() -> Result<(), Box<dyn Error>> {
    let (source, native) = roots()?;
    let started = Instant::now();
    let mut session = AnalysisSession::open(&source).map_err(|error| format!("{error:?}"))?;
    let families = session
        .owner_paths()
        .filter_map(|path| {
            let forms = &session.owner(path)?.forms;
            (!forms.is_empty()).then(|| {
                (
                    path.clone(),
                    forms
                        .iter()
                        .filter_map(|form| form.dialog.clone())
                        .collect::<Vec<_>>(),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    let owners = families.len();
    let mut forms = 0;
    let mut absent_dialogs = 0;
    let mut compared = 0;
    let mut elements = 0;
    let mut attributes = 0;
    let mut columns = 0;
    let mut events = 0;
    let mut commands = 0;
    let mut peak_source_bytes = 0;
    for (owner, mut resources) in families {
        let request = InputRequest::default();
        for path in &resources {
            session
                .load_resource_saved(path, session.revision(), &request)
                .map_err(|error| format!("{error:?}"))?;
        }
        session
            .load_saved(&owner, session.revision(), &request)
            .map_err(|error| format!("{error:?}"))?;
        let model = session.owner(&owner).ok_or("missing_current_form_owner")?;
        for form in &model.forms {
            forms += 1;
            if form.dialog.is_none() {
                if !matches!(&form.state, ModelState::Unavailable(eska::analysis_core::InputFailure::Source(error)) if error.code == "analysis_form_dialog_undeclared")
                {
                    return Err("absent_dialog_not_explicitly_unavailable".into());
                }
                absent_dialogs += 1;
                continue;
            }
            let ModelState::Ready(value) = &form.state else {
                return Err(format!(
                    "form_unavailable:{}:{:?}",
                    form.native_path.as_str(),
                    form.state
                )
                .into());
            };
            let CanonicalMetadata::Form(current) = value.as_ref() else {
                return Err("unexpected_canonical_form_kind".into());
            };
            let prefix = form
                .native_path
                .as_str()
                .strip_suffix(".xml")
                .ok_or("invalid_form_descriptor_path")?;
            let reference = bsl_metadata::xml_parser::parse_form_from_bsl_path(
                &native.join(format!("{prefix}/Ext/Form/Module.bsl")),
            )?;
            if **current != reference {
                return Err(format!("form_model_mismatch:{}", form.native_path.as_str()).into());
            }
            compared += 1;
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
        peak_source_bytes = peak_source_bytes.max(session.resources().total_source_bytes);
        resources.push(owner);
        session
            .invalidate_saved_inputs(&resources, session.revision())
            .map_err(|error| format!("{error:?}"))?;
        if session.resources().total_source_bytes != 0 {
            return Err("source_reservations_not_released".into());
        }
    }
    if compared == 0 {
        return Err("no_nested_xml_forms".into());
    }
    println!(
        "{}",
        serde_json::json!({"nested_forms":forms,"owners":owners,"xml_forms_compared":compared,"absent_xml_dialogs":absent_dialogs,"elements":elements,"attributes":attributes,"attribute_columns":columns,"event_handlers":events,"command_handlers":commands,"model_parity":true,"main_xml_generated":false,"peak_current_source_bytes":peak_source_bytes,"resources":session.resources(),"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}

/// The example accepts explicit independent ESKA and native reference roots.
fn roots() -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1).map(PathBuf::from);
    match (arguments.next(), arguments.next(), arguments.next()) {
        (Some(source), Some(native), None) => Ok((source, native)),
        _ => Err("expected_source_and_native_roots".into()),
    }
}
