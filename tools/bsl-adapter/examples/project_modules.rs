//! Compare the canonical module adapter with the pinned kernel's native parser on a real corpus.

use std::{error::Error, fs, path::PathBuf, time::Instant};

use eska::project_core::{
    MetadataKind, metadata_parser::PropertiesMode, source_format::SourceReader,
};

/// Read declared module owners only and report exact model parity without changing either tree.
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
    let mut modules = 0;
    for path in reader
        .owner_paths()
        .filter(|path| path.as_str().starts_with("CommonModules/"))
    {
        let snapshot = reader.read_metadata(path, PropertiesMode::All)?;
        for document in &snapshot.documents {
            let object = document
                .descriptor
                .objects
                .first()
                .ok_or("missing_module_root")?;
            if object.metadata.kind() != MetadataKind::CommonModule {
                return Err("unexpected_module_kind".into());
            }
            if !document.descriptor.diagnostics.is_empty() {
                return Err("partial_module_projection".into());
            }
            let converted = eska_bsl_adapter::common_module(object)
                .map_err(|error| format!("{}: {error:?}", path.as_str()))?;
            let xml = fs::read_to_string(native.join(document.path.as_path()))?;
            let reference = bsl_metadata::xml_parser::parse_common_module_xml(&xml)?;
            if converted != reference {
                return Err(format!("module_model_mismatch:{}", path.as_str()).into());
            }
            modules += 1;
        }
    }
    if modules == 0 {
        return Err("no_common_modules".into());
    }
    println!(
        "{}",
        serde_json::json!({"modules":modules,"model_parity":true,"xml_exported":false,"trees_changed":false,"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}
