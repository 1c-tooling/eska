//! Stateless patch process requests bind public metadata to exact immutable source projections.

use super::{PatchError, PatchPlan, materialize, plan};
use crate::project::discovery;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, path::Path};

/// Read a complete plan without writing a transfer file or requiring a native installation.
///
/// # Errors
/// Returns the same project/Git/support-gate failures as direct planning.
pub fn plan_reply(root: &Path, base: Option<&str>) -> Result<Value, PatchError> {
    let plan = inspect(root, base)?;
    let fingerprint = fingerprint(&plan)?;
    Ok(json!({"plan":plan.description,"fingerprint":fingerprint,
        "platform_version":plan.platform_version.as_ref().map(crate::project::build::PlatformVersion::as_str)}))
}

/// Recheck caller intent and current state before filling only the caller's new empty namespace.
///
/// # Errors
/// Refuses stale plans, dirty repositories, collisions and incomplete snapshot publication.
pub fn prepare_reply(
    root: &Path,
    base: Option<&str>,
    workspace: &Path,
    expected: &str,
) -> Result<Value, PatchError> {
    let plan = inspect(root, base)?;
    let fingerprint = fingerprint(&plan)?;
    if fingerprint != expected {
        return Err(PatchError::new("plan-changed", &plan.description.head));
    }
    materialize(&plan, workspace)?;
    Ok(json!({"prepared":true,"fingerprint":fingerprint}))
}

/// Keep project discovery identical at both protocol boundaries.
fn inspect(root: &Path, base: Option<&str>) -> Result<PatchPlan, PatchError> {
    let project = discovery::discover(root)
        .map_err(|error| PatchError::new("project", format!("{error:?}")))?;
    let plan = plan(&project, base)?;
    plan.description.validate()?;
    Ok(plan)
}

/// Include materialized bytes and settings, rather than trusting only a branch or display label.
fn fingerprint(plan: &PatchPlan) -> Result<String, PatchError> {
    let mut digest = Sha256::new();
    part(&mut digest, b"eska-patch-transfer-1");
    part(
        &mut digest,
        &serde_json::to_vec(&plan.description)
            .map_err(|error| PatchError::new("io", error.to_string()))?,
    );
    part(
        &mut digest,
        plan.repository_root.as_os_str().as_encoded_bytes(),
    );
    part(
        &mut digest,
        plan.platform_version
            .as_ref()
            .map_or("", crate::project::build::PlatformVersion::as_str)
            .as_bytes(),
    );
    for (path, id) in &plan.source_files {
        part(&mut digest, path.as_bytes());
        part(&mut digest, id.as_bytes());
    }
    for module in &plan.modules {
        part(&mut digest, module.name.as_bytes());
        part(&mut digest, module.descriptor.as_bytes());
        part(&mut digest, module.code.as_bytes());
    }
    let mut encoded = String::with_capacity(64);
    for byte in &digest.finalize() {
        // Formatting into String is infallible; no allocation error is exposed as fmt::Error.
        let _ = write!(encoded, "{byte:02x}");
    }
    Ok(encoded)
}

/// Length prefixes keep adjacent arbitrary strings unambiguous without relying on delimiters.
fn part(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
}
