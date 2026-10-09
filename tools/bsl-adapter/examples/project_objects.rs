//! Compare the object semantics and count localized-caption differences on read-only real sources.

use std::{collections::BTreeMap, error::Error, fs, path::PathBuf, time::Instant};

use bsl_metadata::{MetadataObject, xml_parser};
use eska::project_core::{
    MetadataKind, metadata_parser::PropertiesMode, source_format::SourceReader,
};
use eska_bsl_adapter::AdapterIssue;

/// Audit only main descriptors; forms, module bodies and predefined resources have separate owners.
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
    let mut kinds = BTreeMap::<String, usize>::new();
    let mut unsupported = BTreeMap::<String, usize>::new();
    let mut attributes = 0;
    let mut sections = 0;
    let mut columns = 0;
    let mut caption_differences = 0;
    for path in reader.owner_paths().filter(|path| {
        ["Catalogs/", "Documents/", "DataProcessors/", "Reports/"]
            .iter()
            .any(|prefix| path.as_str().starts_with(prefix))
    }) {
        let snapshot = reader.read_metadata(path, PropertiesMode::All)?;
        for document in snapshot
            .documents
            .iter()
            .filter(|document| document.path.as_path().components().count() == 2)
        {
            let root = document
                .descriptor
                .objects
                .first()
                .ok_or("missing_object_root")?;
            let mut converted = match eska_bsl_adapter::metadata_object(&document.descriptor) {
                Ok(value) => value,
                Err(error)
                    if matches!(
                        error.issue,
                        AdapterIssue::UnsupportedType { .. }
                            | AdapterIssue::UnsupportedTypeField { .. }
                            | AdapterIssue::UnsupportedStandardAttribute { .. }
                            | AdapterIssue::UnsupportedPropertyValue { .. }
                    ) =>
                {
                    *unsupported.entry(format!("{:?}", error.issue)).or_default() += 1;
                    continue;
                }
                Err(error) => return Err(format!("{}:{error:?}", path.as_str()).into()),
            };
            let xml = fs::read_to_string(native.join(document.path.as_path()))?;
            let mut reference = match root.metadata.kind() {
                MetadataKind::Catalog => xml_parser::parse_catalog_xml(&xml),
                MetadataKind::Document => xml_parser::parse_document_xml(&xml),
                MetadataKind::DataProcessor => xml_parser::parse_data_processor_xml(&xml),
                MetadataKind::Report => xml_parser::parse_report_xml(&xml),
                _ => return Err("wrong_object_kind".into()),
            }?;
            caption_differences += compare_captions(&converted, &reference)?;
            for model in [&mut converted, &mut reference] {
                for section in &mut model.tabular_sections {
                    section.set_synonym(None);
                }
            }
            if converted != reference {
                return Err(format!("object_semantic_model_mismatch:{}", path.as_str()).into());
            }
            *kinds
                .entry(root.metadata.kind().as_str().into())
                .or_default() += 1;
            attributes += converted.attributes.len();
            sections += converted.tabular_sections.len();
            columns += converted
                .tabular_sections
                .iter()
                .map(|section| section.attributes().len())
                .sum::<usize>();
        }
    }
    if kinds.is_empty() {
        return Err("no_supported_object_models".into());
    }
    let complete = unsupported.is_empty();
    println!(
        "{}",
        serde_json::json!({"objects_by_kind":kinds,"attributes":attributes,"tabular_sections":sections,"tabular_columns":columns,"caption_differences":caption_differences,"comparison":"all-fields-except-tabular-caption","semantic_model_parity":true,"unsupported":unsupported,"object_scope_complete":complete,"xml_exported":false,"seconds":started.elapsed().as_secs_f64()})
    );
    if !complete {
        std::process::exit(2);
    }
    Ok(())
}

/// Only the known caption defect may differ; all real section identities must already agree.
fn compare_captions(
    converted: &MetadataObject,
    reference: &MetadataObject,
) -> Result<usize, Box<dyn Error>> {
    if converted.tabular_sections.len() != reference.tabular_sections.len() {
        return Err("tabular_section_count_mismatch".into());
    }
    let mut differences = 0;
    for (actual, native) in converted
        .tabular_sections
        .iter()
        .zip(&reference.tabular_sections)
    {
        if actual.uuid() != native.uuid() {
            return Err("tabular_section_identity_mismatch".into());
        }
        if actual.synonym() != native.synonym() {
            if native.synonym().is_some_and(|text| !text.trim().is_empty()) {
                return Err("unexpected_native_tabular_caption".into());
            }
            differences += 1;
        }
    }
    Ok(differences)
}
