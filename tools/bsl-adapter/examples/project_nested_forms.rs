//! Compare nested form declarations/dialogs while keeping one enclosing owner snapshot at a time.

use std::{collections::BTreeMap, error::Error, path::PathBuf, time::Instant};

use eska::project_core::{
    MetadataKind, metadata_parser::PropertiesMode, source_format::SourceReader,
};

/// Full descriptor identities prevent equal short names in different owners from colliding.
fn main() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let [source, native] = arguments.as_slice() else {
        return Err("expected_source_and_native_roots".into());
    };
    let started = Instant::now();
    let reader = SourceReader::open(source)?;
    let mut families = BTreeMap::new();
    let mut absent_dialogs = 0;
    let mut forms = 0;
    for path in reader.form_paths() {
        let form = reader.declared_form(path).ok_or("missing_form_identity")?;
        if form.kind != MetadataKind::Form {
            continue;
        }
        forms += 1;
        if form.dialog.is_none() {
            absent_dialogs += 1;
            continue;
        }
        families
            .entry(form.owner)
            .or_insert_with(Vec::new)
            .push(form);
    }
    let owners = families.len();
    let mut compared = 0;
    let mut elements = 0;
    let mut attributes = 0;
    let mut columns = 0;
    let mut events = 0;
    let mut commands = 0;
    for (owner, forms) in families {
        let snapshot = reader.read_metadata(owner, PropertiesMode::All)?;
        for form in forms {
            let document = snapshot
                .documents
                .iter()
                .find(|document| &document.path == form.native_descriptor)
                .ok_or("missing_current_form_descriptor")?;
            let resource = form.dialog.ok_or("missing_declared_form_resource")?;
            let text = reader.read_form_dialog_text(resource, 64 * 1024 * 1024)?;
            let data = bsl_metadata::xml_parser::parse_form_xml(&text)?;
            let current = eska_bsl_adapter::form(&document.descriptor, data)
                .map_err(|error| format!("{}:{error:?}", form.native_descriptor.as_str()))?;
            let prefix = form
                .native_descriptor
                .as_str()
                .strip_suffix(".xml")
                .ok_or("invalid_form_descriptor_path")?;
            let reference = bsl_metadata::xml_parser::parse_form_from_bsl_path(
                &native.join(format!("{prefix}/Ext/Form/Module.bsl")),
            )?;
            if current != reference {
                return Err(
                    format!("form_model_mismatch:{}", form.native_descriptor.as_str()).into(),
                );
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
    }
    if forms == 0 {
        return Err("no_nested_forms".into());
    }
    println!(
        "{}",
        serde_json::json!({"nested_forms":forms,"compared_forms":compared,"absent_xml_dialogs":absent_dialogs,"enclosing_owners_read_once":owners,"elements":elements,"attributes":attributes,"attribute_columns":columns,"event_handlers":events,"command_handlers":commands,"model_parity":true,"main_xml_generated":false,"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}
