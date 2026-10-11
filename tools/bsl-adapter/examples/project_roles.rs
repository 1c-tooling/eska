//! Compare role identities and preserved rights with the pinned native reader on read-only corpora.

use std::{error::Error, fs, path::PathBuf, time::Instant};

use eska::project_core::{
    MetadataKind, metadata_parser::PropertiesMode, source_format::SourceReader,
};

/// Read only declared role inputs, without generating a main XML descriptor or changing source bytes.
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
    let mut roles = 0;
    let mut rights_resources = 0;
    let mut references = 0;
    let mut restrictions = 0;
    for path in reader
        .owner_paths()
        .filter(|path| path.as_str().starts_with("Roles/"))
    {
        let snapshot = reader.read_metadata(path, PropertiesMode::All)?;
        let [document] = snapshot.documents.as_slice() else {
            return Err("unexpected_nested_role_descriptor".into());
        };
        let object = document
            .descriptor
            .objects
            .first()
            .ok_or("missing_role_root")?;
        if object.metadata.kind() != MetadataKind::Role {
            return Err("unexpected_role_kind".into());
        }
        let resource = reader.role_rights_path(path);
        let rights = resource
            .map(|path| reader.read_role_rights_text(path, 64 * 1024 * 1024))
            .transpose()?;
        // A declared malformed resource is an error, never the legacy reader's default role data.
        let data = rights
            .as_deref()
            .map(bsl_metadata::xml_parser::parse_rights_xml)
            .transpose()?
            .unwrap_or_default();
        let current = eska_bsl_adapter::role(&document.descriptor, data)
            .map_err(|error| format!("{}: {error:?}", path.as_str()))?;
        let main_xml = fs::read_to_string(native.join(document.path.as_path()))?;
        let native_rights = resource
            .map(|path| fs::read_to_string(native.join(path.as_path())))
            .transpose()?;
        let reference = bsl_metadata::parse_role_from_texts(&main_xml, native_rights.as_deref())
            .ok_or("invalid_native_role")?;
        if current != reference {
            return Err(format!("role_model_mismatch:{}", path.as_str()).into());
        }
        roles += 1;
        rights_resources += usize::from(resource.is_some());
        references += current.objects().len();
        restrictions += current
            .objects()
            .iter()
            .map(|object| object.restrictions.len())
            .sum::<usize>();
    }
    if roles == 0 {
        return Err("no_roles".into());
    }
    println!(
        "{}",
        serde_json::json!({"roles":roles,"rights_resources":rights_resources,
        "object_references":references,"rls_conditions":restrictions,"model_parity":true,
        "main_xml_generated":false,"seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}
