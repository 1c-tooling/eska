//! Compare main subsystem models with the pinned native reader on read-only corpora.

use std::{error::Error, fs, path::PathBuf, time::Instant};

use eska::project_core::{
    MetadataKind, metadata_parser::PropertiesMode, source_format::SourceReader,
};

/// Inspect declared subsystems without XML export, platform invocation or source changes.
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
    let mut subsystems = 0;
    for path in reader
        .owner_paths()
        .filter(|path| path.as_str().starts_with("Subsystems/"))
    {
        let snapshot = reader.read_metadata(path, PropertiesMode::All)?;
        // SourceReader orders the main descriptor first; nested coverage is checked by the SDK.
        for document in snapshot.documents.iter().take(1) {
            let object = document
                .descriptor
                .objects
                .first()
                .ok_or("missing_subsystem_root")?;
            if object.metadata.kind() != MetadataKind::Subsystem
                || !document.descriptor.diagnostics.is_empty()
            {
                return Err("incomplete_subsystem_projection".into());
            }
            let converted = eska_bsl_adapter::subsystem(&document.descriptor)
                .map_err(|error| format!("{}: {error:?}", path.as_str()))?;
            let xml = fs::read_to_string(native.join(document.path.as_path()))?;
            let reference = bsl_metadata::xml_parser::parse_subsystem_xml(&xml)?;
            if converted != reference {
                return Err(format!("subsystem_model_mismatch:{}", path.as_str()).into());
            }
            subsystems += 1;
        }
    }
    if subsystems == 0 {
        return Err("no_subsystems".into());
    }
    println!(
        "{}",
        serde_json::json!({"subsystems":subsystems,"model_parity":true,"xml_exported":false,"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}
