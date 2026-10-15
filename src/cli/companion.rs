//! Installed source/project and native/build companions share one concrete discovery/protocol boundary.

pub(super) mod inherited;
mod probe;
#[cfg(test)]
mod tests;

use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

use crate::project_core::ProjectType;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Project,
    Build,
    Check,
}

impl Kind {
    /// Executable and JSON tool identifiers are locale-independent and have one source.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Project => "eska-project",
            Self::Build => "eska-build",
            Self::Check => "eska-check",
        }
    }
}

/// Each current compound operation checks only the capabilities it actually consumes.
#[derive(Clone, Copy, Debug)]
pub(super) enum Capability {
    New(ProjectType),
    ImportQueries,
    Init,
    Unpack,
    SourceImport(ProjectType),
    Check(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Error {
    Missing,
    Unavailable,
    ProbeFailed,
    ProbeTimeout,
    ProbeOversized,
    Incompatible,
}

/// A resolved executable is never silently replaced with a PATH fallback after a failed probe.
pub(super) struct Tool {
    kind: Kind,
    path: PathBuf,
}

impl Tool {
    /// Prefer the bundled sibling, then the first regular executable candidate on PATH.
    pub(super) fn discover(kind: Kind) -> Result<Self, Error> {
        let name = format!("{}{}", kind.name(), env::consts::EXE_SUFFIX);
        let current = env::current_exe().map_err(|_| Error::Unavailable)?;
        let sibling = current.parent().ok_or(Error::Unavailable)?.join(&name);
        let path = if sibling.is_file() {
            Some(sibling)
        } else {
            env::var_os("PATH").and_then(|paths| {
                env::split_paths(&paths)
                    .map(|directory| directory.join(&name))
                    .find(|path| path.is_file())
            })
        }
        .ok_or(Error::Missing)?;
        Ok(Self { kind, path })
    }

    /// Exact package/protocol agreement precedes staging, project claim and workspace file writes.
    pub(super) fn verify(&self, locale: &str, capability: Capability) -> Result<(), Error> {
        let bytes = probe::capture(&self.path, locale, self.kind)?;
        let report: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Incompatible)?;
        validate(&report, self.kind, capability)
    }

    /// Compound callers choose captured streams and retain control over their own temporary resources.
    pub(super) fn command(&self) -> Command {
        Command::new(&self.path)
    }
}

/// Creation capability fields are verified as typed arrays/scalars, never by localized help output.
fn validate(value: &Value, kind: Kind, capability: Capability) -> Result<(), Error> {
    if value["schema_version"] != 1
        || value["tool"] != kind.name()
        || value["command"] != "capabilities"
        || value["status"] != "success"
        || match kind {
            Kind::Check => {
                value["data"]["source_core_version"] != env!("CARGO_PKG_VERSION")
                    || value["data"]["version"].as_str().is_none_or(str::is_empty)
            }
            Kind::Project | Kind::Build => value["data"]["version"] != env!("CARGO_PKG_VERSION"),
        }
        || value["data"]["protocol_version"] != 1
    {
        return Err(Error::Incompatible);
    }
    let data = &value["data"];
    let available = match (kind, capability) {
        (Kind::Project, Capability::New(kind)) => {
            contains(data, "commands", "new")
                && contains(data, "new_project_types", kind.as_str())
                && data["new_source_layout"] == "queries"
                && contains(data, "initial_ancestor_layouts", "queries")
        }
        (Kind::Project, Capability::ImportQueries) => {
            contains(data, "commands", "import")
                && contains(data, "import_layouts", "queries")
                && contains(data, "initial_ancestor_layouts", "queries")
                && ["configuration", "extension", "processing", "report"]
                    .iter()
                    .all(|kind| contains(&data["import_layout_project_types"], "queries", kind))
        }
        (Kind::Project, Capability::Init) => {
            contains(data, "commands", "init")
                && data["initialization_source_layout"] == "queries"
                && data["initialization_converts_source"] == false
                && ["configuration", "extension", "processing", "report"]
                    .iter()
                    .all(|kind| contains(data, "initialization_project_types", kind))
        }
        (Kind::Build, Capability::Unpack) => {
            contains(data, "commands", "unpack")
                && data["unpack"] == true
                && ["configuration", "extension", "processing", "report"]
                    .iter()
                    .all(|kind| contains(data, "designer_artifact_types", kind))
        }
        (Kind::Build, Capability::SourceImport(kind)) => {
            contains(data, "commands", "import")
                && contains(data, "eska_artifact_types", kind.as_str())
                && contains(data, "source_import_formats", "eska")
                && data["source_import_mode"] == "preview_then_explicit_write"
                && data["source_import_ancestor"] == "independently_confirmed_sha256"
                && data["source_import_recovery"] == "eska-project recover-import"
        }
        (Kind::Check, Capability::Check(profile)) => {
            contains(data, "commands", "check")
                && contains(data, "profiles", profile)
                && contains(data, "source_formats", "eska")
                && data["report_schema_version"] == 1
        }
        _ => false,
    };
    if available {
        Ok(())
    } else {
        Err(Error::Incompatible)
    }
}

/// Missing arrays, non-string values and substring lookalikes cannot satisfy a capability requirement.
fn contains(data: &Value, key: &str, expected: &str) -> bool {
    data[key]
        .as_array()
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
}
