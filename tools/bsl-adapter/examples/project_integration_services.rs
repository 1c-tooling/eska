//! Compare complete integration models with the pinned native reader on read-only corpora.

use std::{error::Error, fs, path::PathBuf, time::Instant};

use eska::project_core::{
    MetadataKind, metadata_parser::PropertiesMode, source_format::SourceReader,
};

/// Inspect declared channel bindings without XML export, platform invocation or source changes.
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
    let mut services = 0;
    for path in reader
        .owner_paths()
        .filter(|path| path.as_str().starts_with("IntegrationServices/"))
    {
        let snapshot = reader.read_metadata(path, PropertiesMode::All)?;
        for document in &snapshot.documents {
            let object = document
                .descriptor
                .objects
                .first()
                .ok_or("missing_integration_root")?;
            if object.metadata.kind() != MetadataKind::IntegrationService
                || !document.descriptor.diagnostics.is_empty()
            {
                return Err("incomplete_integration_projection".into());
            }
            let converted = eska_bsl_adapter::integration_service(&document.descriptor)
                .map_err(|error| format!("{}: {error:?}", path.as_str()))?;
            let xml = fs::read_to_string(native.join(document.path.as_path()))?;
            let name = document
                .path
                .as_path()
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or("missing_integration_native_name")?;
            let reference = bsl_metadata::xml_parser::parse_integration_service_xml(&xml, name)?;
            if converted != reference {
                return Err(format!("integration_model_mismatch:{}", path.as_str()).into());
            }
            services += 1;
        }
    }
    if services == 0 {
        return Err("no_integration_services".into());
    }
    println!(
        "{}",
        serde_json::json!({"integration_services":services,"model_parity":true,"xml_exported":false,"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}
