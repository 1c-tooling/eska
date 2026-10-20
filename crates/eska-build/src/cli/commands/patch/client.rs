//! Strict process adapter keeps Git planning and immutable projection in the VCS owner.

use crate::cli::localization::Localizer;
use eska_build_runtime::build::PlatformVersion;
use eska_project_core::patch::{PatchDescription, PatchError};
use eska_tool_runtime::{Component, Error, QueryPolicy, query_with_policy};
use serde::Deserialize;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) struct Client {
    binary: PathBuf,
    root: PathBuf,
    options: Vec<OsString>,
}

pub(super) struct Plan {
    pub description: PatchDescription,
    pub fingerprint: String,
    pub platform_version: Option<PlatformVersion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanReply {
    plan: PatchDescription,
    fingerprint: String,
    platform_version: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedReply {
    prepared: bool,
    fingerprint: String,
}

impl Client {
    /// Bind one executable and base selection before creating any native workspace.
    pub fn new(
        root: &Path,
        base: Option<&str>,
        binary: Option<&Path>,
        localizer: &Localizer,
    ) -> Result<Self, PatchError> {
        let binary = binary
            .map_or_else(
                || eska_tool_runtime::resolve(Component::Vcs),
                |path| Ok(path.to_owned()),
            )
            .map_err(|error| present(error, localizer))?;
        let options = base.map_or_else(Vec::new, |base| vec!["--base".into(), base.into()]);
        Ok(Self {
            binary,
            root: root.to_owned(),
            options,
        })
    }

    /// Accept only a complete validated plan; large metadata has its own finite output limit.
    pub fn plan(&self, localizer: &Localizer) -> Result<Plan, PatchError> {
        let value = query_with_policy(
            Component::Vcs,
            &self.binary,
            "patch_plan",
            &self.root,
            &self.options,
            QueryPolicy {
                timeout: Duration::from_secs(30),
                output_limit: 64 * 1024 * 1024,
            },
        )
        .map_err(|error| present(error, localizer))?;
        if value.as_object().is_none_or(|object| object.len() != 3) {
            return Err(present(Error::InvalidResponse, localizer));
        }
        let reply: PlanReply = serde_json::from_value(value)
            .map_err(|_| present(Error::InvalidResponse, localizer))?;
        if reply.fingerprint.len() != 64
            || !reply
                .fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(present(Error::InvalidResponse, localizer));
        }
        reply
            .plan
            .validate()
            .map_err(|_| present(Error::InvalidResponse, localizer))?;
        let version = reply
            .platform_version
            .map(|version| PlatformVersion::parse(&version))
            .transpose()
            .map_err(|_| present(Error::InvalidResponse, localizer))?;
        Ok(Plan {
            description: reply.plan,
            fingerprint: reply.fingerprint,
            platform_version: version,
        })
    }

    /// Recheck immutable intent before VCS fills only the caller's empty owned workspace.
    pub fn prepare(
        &self,
        fingerprint: &str,
        workspace: &Path,
        localizer: &Localizer,
    ) -> Result<(), PatchError> {
        let mut options = self.options.clone();
        options.extend([
            "--workspace".into(),
            workspace.as_os_str().to_owned(),
            "--expected".into(),
            fingerprint.into(),
        ]);
        let value = query_with_policy(
            Component::Vcs,
            &self.binary,
            "prepare_patch",
            &self.root,
            &options,
            QueryPolicy {
                timeout: Duration::from_mins(30),
                ..QueryPolicy::default()
            },
        )
        .map_err(|error| present(error, localizer))?;
        let reply: PreparedReply = serde_json::from_value(value)
            .map_err(|_| present(Error::InvalidResponse, localizer))?;
        if !reply.prepared || reply.fingerprint != fingerprint {
            return Err(present(Error::InvalidResponse, localizer));
        }
        Ok(())
    }
}

/// Keep domain codes stable while localizing transport and process failures in presentation.
fn present(error: Error, localizer: &Localizer) -> PatchError {
    match error {
        Error::Rejected { code, detail, .. } => PatchError::new(code, detail.unwrap_or_default()),
        Error::Cancelled => PatchError::new("tool", localizer.text("component-cancelled")),
        error => PatchError::new(
            "component",
            eska_presentation::components::present(&error, localizer),
        ),
    }
}
