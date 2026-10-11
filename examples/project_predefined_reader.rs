//! Compare declared predefined-data bytes with native files without reading unrelated owner payloads.

use std::{collections::BTreeMap, error::Error, fs, path::PathBuf};

use eska::project_core::source_format::SourceReader;

/// The shared reader supplies current bytes; native files provide an independent verbatim reference.
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1).map(PathBuf::from);
    let (Some(source), Some(native), None) = (arguments.next(), arguments.next(), arguments.next())
    else {
        return Err("expected_source_and_native_roots".into());
    };
    let reader = SourceReader::open(&source)?;
    let mut resources = 0;
    let mut bytes = 0;
    let mut kinds = BTreeMap::<&str, usize>::new();
    for resource in reader.predefined_paths() {
        let owner = reader
            .predefined_owner(resource)
            .ok_or("missing_predefined_owner")?;
        let identity = reader.declared_owner(owner)?;
        if reader.predefined_path(owner) != Some(resource) {
            return Err("inconsistent_predefined_identity".into());
        }
        let current = reader.read_predefined_text(resource, 64 * 1024 * 1024)?;
        let reference = fs::read(native.join(resource.as_path()))?;
        if current.as_bytes() != reference {
            return Err("predefined_current_bytes_mismatch".into());
        }
        resources += 1;
        bytes += current.len();
        *kinds.entry(identity.kind.as_str()).or_default() += 1;
    }
    if resources == 0 {
        return Err("no_declared_predefined_resources".into());
    }
    println!(
        "{}",
        serde_json::json!({"resources":resources,"source_bytes":bytes,"kinds":kinds,"native_bytes_equal":true,"non_root_owner_payloads_read":0,"main_xml_generated":false})
    );
    Ok(())
}
