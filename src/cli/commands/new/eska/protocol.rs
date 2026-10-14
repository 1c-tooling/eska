//! Creation receipts report candidates and original manifest bytes; they never approve source policy.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::project_core::{
    ProjectName,
    config::ProjectConfig,
    source_format::{AcceptedRef, ImportAncestorRef},
};

use super::process::Captured;

#[cfg(test)]
mod tests;

/// A machine report identifies one exact operation, never an already printed intermediate success.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cli) enum Operation {
    New,
    Import,
    Init,
}

impl Operation {
    /// Preserve the direct tool's stable command identifiers in the compound protocol.
    pub(in crate::cli) const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Import => "import",
            Self::Init => "init",
        }
    }
}

/// These references are creation candidates; an application/CI must select later approval itself.
pub(in crate::cli) struct Created {
    pub(in crate::cli) root: PathBuf,
    pub(in crate::cli) name: ProjectName,
    pub(in crate::cli) source_relative: PathBuf,
    pub(in crate::cli) manifest_sha256: String,
    pub(in crate::cli) accepted: AcceptedRef,
    pub(in crate::cli) ancestor: ImportAncestorRef,
}

/// A child error is distinct from an invalid acknowledgment after possible project publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cli) enum Error {
    ChildFailed,
    ResponseInvalid,
    ManifestChanged,
    SourceChanged,
}

/// Verify the acknowledgment's paths and pin syntax before permitting parent-only finalization.
pub(in crate::cli) fn created(
    output: &Captured,
    operation: Operation,
    expected_root: &Path,
    expected_name: &ProjectName,
    source_relative: &Path,
) -> Result<Created, Error> {
    if !output.status.success() {
        return Err(Error::ChildFailed);
    }
    if output.oversized {
        return Err(Error::ResponseInvalid);
    }
    let value: Value =
        serde_json::from_slice(&output.stdout).map_err(|_| Error::ResponseInvalid)?;
    let data = &value["data"];
    if value["schema_version"] != 1
        || value["tool"] != "eska-project"
        || value["command"] != operation.as_str()
        || value["status"] != "success"
        || data["root"].as_str().map(Path::new) != Some(expected_root)
        || data["name"] != expected_name.as_str()
        || data["source_format"] != "eska"
        || data["source_layout"] != "queries"
        || data["accepted_base"] != ".eska/accepted"
        || data["source"].as_str().map(Path::new)
            != Some(expected_root.join(source_relative).as_path())
    {
        return Err(Error::ResponseInvalid);
    }
    let accepted = digest(data, "accepted_base_sha256")?;
    let ancestor = ImportAncestorRef::from_sha256(&digest(data, "ancestor_sha256")?)
        .map_err(|_| Error::ResponseInvalid)?;
    Ok(Created {
        root: expected_root.to_owned(),
        name: expected_name.clone(),
        source_relative: source_relative.to_owned(),
        manifest_sha256: digest(data, "manifest_sha256")?,
        accepted: AcceptedRef { sha256: accepted },
        ancestor,
    })
}

/// Root/manifest aliases and post-child edits refuse finalization instead of becoming new expected bytes.
pub(in crate::cli) fn manifest(created: &Created) -> Result<(String, ProjectConfig), Error> {
    if created
        .root
        .canonicalize()
        .map_err(|_| Error::ManifestChanged)?
        != created.root
    {
        return Err(Error::ManifestChanged);
    }
    let path = created.root.join(crate::project_core::config::FILE_NAME);
    let metadata = fs::symlink_metadata(&path).map_err(|_| Error::ManifestChanged)?;
    if !metadata.file_type().is_file() || metadata.len() > 1_048_576 {
        return Err(Error::ManifestChanged);
    }
    let bytes = fs::read(path).map_err(|_| Error::ManifestChanged)?;
    if digest_bytes(&bytes) != created.manifest_sha256 {
        return Err(Error::ManifestChanged);
    }
    let original = String::from_utf8(bytes).map_err(|_| Error::ManifestChanged)?;
    let config = ProjectConfig::from_toml(&original).map_err(|_| Error::ManifestChanged)?;
    if config.name() != Some(&created.name)
        || config.source() != created.source_relative
        || !matches!(config.configuration().source_format(), crate::project_core::SourceFormat::Eska { accepted_base }
            if accepted_base.as_str() == ".eska/accepted")
    {
        return Err(Error::ManifestChanged);
    }
    Ok((original, config))
}

/// Unfinished creation remains explicit and never authorizes deleting the child's published project.
pub(in crate::cli) fn unfinished(created: &Created, stage: &str, source_verified: bool) -> Value {
    json!({"root":created.root,"stage":stage,"project_created":true,"onboarding_complete":false,
        "source_verified":source_verified,"source_format":"eska","source_layout":"queries",
        "accepted_base_sha256":created.accepted.sha256,"ancestor_sha256":created.ancestor.sha256(),
        "support_approved":false,"destination_removed":false})
}

/// Caller receipts use canonical SHA-256 values; a mutable manifest never supplies their approval.
fn digest(data: &Value, key: &str) -> Result<String, Error> {
    let value = data[key].as_str().ok_or(Error::ResponseInvalid)?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Error::ResponseInvalid);
    }
    Ok(value.to_owned())
}

/// Manifest hashing is transport integrity only, separate from accepted-source or ancestry authority.
pub(in crate::cli) fn digest_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut value, byte| {
            // Writing into a String is infallible; retaining the result satisfies the formatter API.
            let _ = write!(value, "{byte:02x}");
            value
        })
}
