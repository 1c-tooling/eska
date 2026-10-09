//! Compare register models from canonical YAML with the selected kernel's native reader.

use std::{collections::BTreeMap, error::Error, fs, path::PathBuf, time::Instant};

use bsl_metadata::{Register, error::Result as MetadataResult};
use eska::project_core::{
    MetadataKind, metadata_parser::PropertiesMode, source_format::SourceReader,
};

/// Read only declared register owners; no XML export, Git, platform or IDE process is involved.
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
    let mut counts = BTreeMap::new();
    let mut unsupported = BTreeMap::new();
    let mut fields = 0;
    for path in reader.owner_paths().filter(|path| {
        [
            "InformationRegisters/",
            "AccumulationRegisters/",
            "AccountingRegisters/",
            "CalculationRegisters/",
        ]
        .iter()
        .any(|prefix| path.as_str().starts_with(prefix))
    }) {
        let snapshot = reader.read_metadata(path, PropertiesMode::All)?;
        for document in &snapshot.documents {
            let object = document
                .descriptor
                .objects
                .first()
                .ok_or("missing_register_root")?;
            // Nested forms remain separate descriptors and are not a register's record model.
            let parser: fn(&str) -> MetadataResult<Register> = match object.metadata.kind() {
                MetadataKind::InformationRegister => {
                    bsl_metadata::xml_parser::parse_information_register_xml
                }
                MetadataKind::AccumulationRegister => {
                    bsl_metadata::xml_parser::parse_accumulation_register_xml
                }
                MetadataKind::AccountingRegister => {
                    bsl_metadata::xml_parser::parse_accounting_register_xml
                }
                MetadataKind::CalculationRegister => {
                    bsl_metadata::xml_parser::parse_calculation_register_xml
                }
                _ => continue,
            };
            let converted = match eska_bsl_adapter::register(&document.descriptor) {
                Ok(value) => value,
                Err(error) => {
                    let code = match &error.issue {
                        eska_bsl_adapter::AdapterIssue::UnsupportedPropertyValue {
                            name,
                            value,
                        } => format!("{name}.{value}"),
                        eska_bsl_adapter::AdapterIssue::UnsupportedType { .. } => {
                            "unsupported_type".into()
                        }
                        eska_bsl_adapter::AdapterIssue::UnsupportedTypeField { .. } => {
                            "unsupported_type_field".into()
                        }
                        eska_bsl_adapter::AdapterIssue::UnsupportedStandardAttribute { .. } => {
                            "unsupported_standard_attribute".into()
                        }
                        eska_bsl_adapter::AdapterIssue::IncompleteDescriptor => {
                            "incomplete_descriptor".into()
                        }
                        _ => return Err(format!("{}: {error:?}", path.as_str()).into()),
                    };
                    *unsupported.entry(code).or_insert(0_u64) += 1;
                    continue;
                }
            };
            let xml = fs::read_to_string(native.join(document.path.as_path()))?;
            let reference = parser(&xml)?;
            if converted != reference {
                return Err(format!("register_model_mismatch:{}", path.as_str()).into());
            }
            fields += converted.dimensions().len()
                + converted.resources().len()
                + converted.attributes().len();
            *counts
                .entry(object.metadata.kind().as_str())
                .or_insert(0_u64) += 1;
        }
    }
    if counts.is_empty() {
        return Err("no_registers".into());
    }
    println!(
        "{}",
        serde_json::json!({"registers":counts,"fields":fields,"supported_models_equal":true,"metadata_complete":unsupported.is_empty(),"unsupported":unsupported,"xml_exported":false,"seconds":started.elapsed().as_secs_f64()})
    );
    if !unsupported.is_empty() {
        std::process::exit(2);
    }
    Ok(())
}
