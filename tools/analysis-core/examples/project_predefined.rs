//! Compare current predefined items and composed object models with independent native text inputs.

use std::{collections::BTreeMap, error::Error, fs, path::PathBuf};

use eska::{analysis_core::InputRequest, project_core::source_format::SourceReader};
use eska_analysis::{AnalysisSession, ModelState, SessionOptions};
use ide_db::metadata::CanonicalMetadata;

/// Load only selected owners and their current sidecars, then release their saved input reservations.
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1).map(PathBuf::from);
    let (Some(source), Some(native), None) = (arguments.next(), arguments.next(), arguments.next())
    else {
        return Err("expected_source_and_native_roots".into());
    };
    let reader = SourceReader::open(&source)?;
    let resources = reader.predefined_paths().cloned().collect::<Vec<_>>();
    let mut session =
        AnalysisSession::from_reader(SourceReader::open(&source)?, SessionOptions::default())
            .map_err(|error| format!("{error:?}"))?;
    let request = InputRequest::default();
    let mut items = 0;
    let mut compared = 0;
    let mut unavailable = 0;
    let mut peak_source_bytes = 0;
    let mut kinds = BTreeMap::<&str, usize>::new();
    for resource in &resources {
        let owner = reader
            .predefined_owner(resource)
            .ok_or("missing_predefined_owner")?
            .clone();
        let identity = reader.declared_owner(&owner)?;
        let kind = identity.kind;
        let descriptor_path = identity.native_path.clone();
        let current_text = reader.read_predefined_text(resource, 64 * 1024 * 1024)?;
        let native_text = fs::read_to_string(native.join(resource.as_path()))?;
        let current_items = bsl_metadata::xml_parser::try_parse_predefined_xml(&current_text)?;
        let reference_items = bsl_metadata::xml_parser::parse_predefined_xml(&native_text);
        if current_items != reference_items {
            return Err("predefined_item_model_mismatch".into());
        }
        items += current_items.len();
        *kinds.entry(kind.as_str()).or_default() += 1;
        session
            .load_saved(&owner, session.revision(), &request)
            .map_err(|error| format!("{error:?}"))?;
        session
            .load_resource_saved(resource, session.revision(), &request)
            .map_err(|error| format!("{error:?}"))?;
        if let Some(CanonicalMetadata::Object(model)) = session
            .resolve(&owner, session.revision())
            .map_err(|error| format!("{error:?}"))?
        {
            let native_xml = fs::read_to_string(native.join(descriptor_path.as_path()))?;
            let Some(mut reference) = bsl_metadata::parse_metadata_object_from_texts(
                model.mdo_type,
                &native_xml,
                Some(&native_text),
            ) else {
                return Err("missing_native_object_model".into());
            };
            if model.predefined_items != current_items {
                return Err("current_predefined_projection_mismatch".into());
            }
            let mut converted = model.as_ref().clone();
            // Native tabular captions contain a known whitespace bug; exclude only that field.
            for value in [&mut converted, &mut reference] {
                for section in &mut value.tabular_sections {
                    section.set_synonym(None);
                }
            }
            if converted != reference {
                return Err("composed_object_model_mismatch".into());
            }
            compared += 1;
        } else if matches!(
            session.owner(&owner).ok_or("missing_owner_model")?.state,
            ModelState::Invalid { .. }
        ) {
            unavailable += 1;
        } else {
            return Err("unexpected_predefined_model_state".into());
        }
        peak_source_bytes = peak_source_bytes.max(session.resources().total_source_bytes);
        session
            .invalidate_saved_inputs(&[owner, resource.clone()], session.revision())
            .map_err(|error| format!("{error:?}"))?;
        if session.resources().total_source_bytes != 0 {
            return Err("saved_source_reservation_not_released".into());
        }
    }
    if resources.is_empty() {
        return Err("no_declared_predefined_resources".into());
    }
    println!(
        "{}",
        serde_json::json!({"resources":resources.len(),"predefined_items":items,"items_equal_native":true,"composed_objects_equal_native_except_tabular_caption":true,"composed_objects_compared":compared,"current_invalid_owners":unavailable,"kinds":kinds,"peak_selected_owner_source_bytes":peak_source_bytes,"remaining_source_bytes":session.resources().total_source_bytes,"main_xml_generated":false})
    );
    Ok(())
}
