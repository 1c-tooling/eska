//! Compare complete constant/enum kernel models from YAML with native readers on a read-only corpus.

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use eska::project_core::{metadata_parser::PropertiesMode, source_format::SourceReader};

/// Read only declared primitive owners, without native materialization or external programs.
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
    let mut constants = 0;
    let mut enums = 0;
    let mut values = 0;
    for path in reader.owner_paths().filter(|path| {
        path.as_str().starts_with("Constants/") || path.as_str().starts_with("Enums/")
    }) {
        let snapshot = reader.read_metadata(path, PropertiesMode::All)?;
        for document in snapshot.documents.iter().filter(|document| {
            let parent = document.path.as_path().parent();
            parent == Some(Path::new("Constants")) || parent == Some(Path::new("Enums"))
        }) {
            let xml = fs::read_to_string(native.join(document.path.as_path()))?;
            let (converted, reference) = if path.as_str().starts_with("Constants/") {
                constants += 1;
                (
                    eska_bsl_adapter::constant_object(&document.descriptor),
                    bsl_metadata::xml_parser::parse_constant_xml(&xml),
                )
            } else {
                enums += 1;
                (
                    eska_bsl_adapter::enum_object(&document.descriptor),
                    bsl_metadata::xml_parser::parse_enum_xml(&xml),
                )
            };
            let converted = converted.map_err(|error| format!("{}: {error:?}", path.as_str()))?;
            let reference = reference?;
            if converted != reference {
                return Err(format!("primitive_model_mismatch:{}", path.as_str()).into());
            }
            values += converted.enum_values.len();
        }
    }
    if constants == 0 || enums == 0 {
        return Err("missing_primitive_family".into());
    }
    println!(
        "{}",
        serde_json::json!({"constants":constants,"enums":enums,"enum_values":values,"model_parity":true,"xml_exported":false,"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}
