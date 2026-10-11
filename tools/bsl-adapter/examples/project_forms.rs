//! Compare declared common forms against the pinned native reader without editing source corpora.

use std::{error::Error, fs, path::PathBuf, time::Instant};

use eska::project_core::{
    MetadataKind,
    metadata_parser::PropertiesMode,
    source_format::{FileRole, SourceReader},
};

/// Supply only preserved resources belonging to declared common forms, never stray native files.
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
    let mut forms = 0;
    let mut elements = 0;
    let mut attributes = 0;
    let mut columns = 0;
    let mut events = 0;
    let mut commands = 0;
    for path in reader
        .owner_paths()
        .filter(|path| path.as_str().starts_with("CommonForms/"))
    {
        let snapshot = reader.read_metadata(path, PropertiesMode::All)?;
        let [document] = snapshot.documents.as_slice() else {
            return Err("unexpected_nested_common_form".into());
        };
        let object = document
            .descriptor
            .objects
            .first()
            .ok_or("missing_common_form_root")?;
        if object.metadata.kind() != MetadataKind::CommonForm {
            return Err("unexpected_common_form_kind".into());
        }
        let prefix = document
            .path
            .as_str()
            .strip_suffix(".xml")
            .ok_or("invalid_common_form_path")?;
        let expected = format!("{prefix}/Ext/Form.xml");
        let resource = reader
            .manifest()
            .files
            .iter()
            .find(|file| {
                file.role == FileRole::Preserved
                    && file.path.as_str().eq_ignore_ascii_case(&expected)
            })
            .ok_or("missing_declared_form_resource")?;
        let text = fs::read_to_string(source.join(resource.path.as_path()))?;
        let data = bsl_metadata::xml_parser::parse_form_xml(&text)?;
        let current = eska_bsl_adapter::form(&document.descriptor, data)
            .map_err(|error| format!("{}: {error:?}", path.as_str()))?;
        let reference = bsl_metadata::xml_parser::parse_form_from_bsl_path(
            &native.join(format!("{prefix}/Ext/Form/Module.bsl")),
        )?;
        if current != reference {
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
        serde_json::json!({"common_forms":forms,"elements":elements,"attributes":attributes,"attribute_columns":columns,"event_handlers":events,"command_handlers":commands,"model_parity":true,"main_xml_generated":false,"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}
