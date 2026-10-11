//! Compare complete Web models with the pinned native reader on read-only corpora.

use std::{error::Error, fs, path::PathBuf, time::Instant};

use eska::project_core::{
    MetadataKind, metadata_parser::PropertiesMode, source_format::SourceReader,
};

/// Inspect declared operations and parameters without XML export, platform invocation or source changes.
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
        .filter(|path| path.as_str().starts_with("WebServices/"))
    {
        let snapshot = reader.read_metadata(path, PropertiesMode::All)?;
        for document in &snapshot.documents {
            let object = document
                .descriptor
                .objects
                .first()
                .ok_or("missing_web_root")?;
            if object.metadata.kind() != MetadataKind::WebService
                || !document.descriptor.diagnostics.is_empty()
            {
                return Err("incomplete_web_projection".into());
            }
            let converted = eska_bsl_adapter::web_service(&document.descriptor)
                .map_err(|error| format!("{}: {error:?}", path.as_str()))?;
            let xml = fs::read_to_string(native.join(document.path.as_path()))?;
            let name = document
                .path
                .as_path()
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or("missing_web_native_name")?;
            let reference = bsl_metadata::xml_parser::parse_web_service_xml(&xml, name)?;
            if converted != reference {
                return Err(format!("web_model_mismatch:{}", path.as_str()).into());
            }
            services += 1;
        }
    }
    if services == 0 {
        return Err("no_web_services".into());
    }
    println!(
        "{}",
        serde_json::json!({"web_services":services,"model_parity":true,"xml_exported":false,"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}
