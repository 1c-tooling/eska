//! Execute the verified 8.3 pipeline through the existing platform runner.

use std::{ffi::OsString, fs, path::Path};

use super::{ArtifactError, Identity, identity::inspect_identity, io_error, staging::Staging};
use crate::project::build::{Ibcmd, ProcessStream};

/// An owned, unpacked artifact whose sources survive until publication or cancellation.
pub struct PreparedArtifact {
    staging: Staging,
    identity: Identity,
}

impl PreparedArtifact {
    /// Snapshot and unpack a native file, then determine its actual metadata type.
    ///
    /// # Errors
    /// Returns input, platform, cancellation or XML errors before any project is changed.
    pub fn unpack<F>(
        input: &Path,
        parent: &Path,
        tool: &Ibcmd,
        mut output: F,
    ) -> Result<Self, ArtifactError>
    where
        F: FnMut(ProcessStream, &[u8]),
    {
        let input = fs::canonicalize(input).map_err(|error| io_error(input, error))?;
        let extension = input
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "cf" | "cfe" | "epf" | "erf") || !input.is_file() {
            return Err(ArtifactError::UnsupportedFile(input));
        }
        let staging = Staging::create(parent)?;
        let snapshot = staging.path.join(format!("input.{extension}"));
        fs::copy(&input, &snapshot).map_err(|error| io_error(&input, error))?;
        let sources = staging.path.join("sources");
        let unpacked = staging.path.join("unpacked");
        fs::create_dir(&unpacked).map_err(|error| io_error(&unpacked, error))?;
        let export = unpacked.join(
            input
                .file_stem()
                .ok_or_else(|| ArtifactError::UnsupportedFile(input.clone()))?,
        );
        let data = staging.path.join("infobase");
        let pid = staging.path.join("ibcmd.pid");
        tool.begin_interruptible_operation()
            .map_err(ArtifactError::Run)?;
        run(
            tool,
            &pid,
            "create-infobase",
            vec!["infobase".into(), "create".into(), option("--data=", &data)],
            &mut output,
        )?;
        run(
            tool,
            &pid,
            "export",
            vec![
                "infobase".into(),
                "config".into(),
                "export".into(),
                option("--data=", &data),
                option("--file=", &snapshot),
                export.as_os_str().to_owned(),
            ],
            &mut output,
        )?;
        // External objects produce `<path>.xml` plus an optional `<path>/` payload tree.
        // Configurations instead produce a directory containing Configuration.xml.
        let mut external_descriptor = export.as_os_str().to_owned();
        external_descriptor.push(".xml");
        let exported = if Path::new(&external_descriptor).is_file() {
            &unpacked
        } else {
            &export
        };
        fs::rename(exported, &sources).map_err(|error| io_error(exported, error))?;
        let identity = inspect_identity(&sources)?
            .ok_or_else(|| ArtifactError::MissingDescriptor(sources.clone()))?;
        Ok(Self { staging, identity })
    }

    /// Return the identity read from exported XML rather than the artifact filename.
    #[must_use]
    pub const fn identity(&self) -> &Identity {
        &self.identity
    }

    /// Return the isolated source directory ready for publication.
    #[must_use]
    pub fn sources(&self) -> std::path::PathBuf {
        self.staging.path.join("sources")
    }
}

/// Construct one native argument without lossy path conversion or shell interpolation.
fn option(prefix: &str, path: &Path) -> OsString {
    let mut value = OsString::from(prefix);
    value.push(path);
    value
}

/// Preserve native diagnostics and stage information for the presentation layer.
fn run<F>(
    tool: &Ibcmd,
    pid: &Path,
    stage: &'static str,
    arguments: Vec<OsString>,
    output: &mut F,
) -> Result<(), ArtifactError>
where
    F: FnMut(ProcessStream, &[u8]),
{
    let result = tool
        .run_interruptible(arguments, pid, output)
        .map_err(ArtifactError::Run)?;
    if !result.status.success() {
        return Err(ArtifactError::Platform {
            stage,
            output: format!(
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            ),
        });
    }
    Ok(())
}
