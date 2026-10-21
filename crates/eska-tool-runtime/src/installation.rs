//! Managed receipts reject unsupported schemas without hashing a large executable on every startup.

use crate::{Error, contract};
use serde_json::Value;
use std::{fs, io::Read, path::Path};

/// Check declared compatibility and executable size for a managed, canonically named component.
///
/// # Errors
/// Returns an incompatible installation for a malformed receipt, unsupported contract or changed size.
pub fn verify(binary: &Path, tool: &str) -> Result<(), Error> {
    let name = format!("{tool}{}", std::env::consts::EXE_SUFFIX);
    let Some(bin) = binary
        .parent()
        .filter(|path| path.file_name().is_some_and(|name| name == "bin"))
    else {
        return Ok(());
    };
    if binary.file_name() != Some(std::ffi::OsStr::new(&name)) {
        return Ok(());
    }
    let Some(root) = bin.parent() else {
        return Ok(());
    };
    let directory = root.join("components");
    let receipt = directory.join(format!("{tool}.json"));
    let invalid = || Error::Incompatible {
        tool: tool.to_owned(),
        receipt: receipt.clone(),
    };
    match fs::symlink_metadata(&receipt) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(invalid()),
        Ok(info) if !info.is_file() || info.len() > 4 * 1024 * 1024 => return Err(invalid()),
        Ok(_) => {}
    }
    if !fs::symlink_metadata(directory).is_ok_and(|info| info.is_dir()) {
        return Err(invalid());
    }
    let mut data = Vec::new();
    fs::File::open(&receipt)
        .map_err(|_| invalid())?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|_| invalid())?;
    if data.len() > 4 * 1024 * 1024 {
        return Err(invalid());
    }
    let value: Value = serde_json::from_slice(&data).map_err(|_| invalid())?;
    let package = &value["package"];
    let metadata = &package["contract"];
    let valid = value.as_object().is_some_and(|object| object.len() == 4)
        && value["schema_version"] == 1
        && value["method"] == "component-archive"
        && sha256(&value["archive_sha256"])
        && package.as_object().is_some_and(|object| object.len() == 6)
        && package["schema_version"] == 1
        && package["tool"] == tool
        && package["version"] == metadata["version"]
        && package["target"] == contract::TARGET
        && contract::compatible(metadata, tool)
        && package["files"].as_array().is_some_and(|files| {
            let matches: Vec<_> = files
                .iter()
                .filter(|item| item["path"] == format!("bin/{name}"))
                .collect();
            (2..=8192).contains(&files.len())
                && matches.len() == 1
                && matches[0]["executable"] == true
                && sha256(&matches[0]["sha256"])
                && fs::metadata(binary)
                    .is_ok_and(|info| matches[0]["size"].as_u64() == Some(info.len()))
        });
    if valid { Ok(()) } else { Err(invalid()) }
}

/// Digests remain canonical even though content ownership is checked by the installer before mutation.
fn sha256(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.len() == 64
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
