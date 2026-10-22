//! Execute the verified 8.3 pipeline through the existing platform runner.

use std::{ffi::OsString, fs, path::Path};

use super::{ArtifactError, Identity, PublicationError, identity::inspect_identity, io_error};
use crate::build::{Ibcmd, ProcessStream};
use eska_project_core::artifact::Staging;

/// An owned, unpacked artifact whose sources survive until publication or cancellation.
pub struct PreparedArtifact {
    staging: Staging,
    identity: Identity,
}

impl PreparedArtifact {
    /// Unpack near the eventual destination into ignored storage and inspect metadata identity.
    ///
    /// # Errors
    /// Returns input, platform, cancellation or XML errors before any project is changed.
    pub fn unpack<F>(
        input: &Path,
        destination: &Path,
        tool: &Ibcmd,
        mut output: F,
    ) -> Result<Self, ArtifactError>
    where
        F: FnMut(ProcessStream, &[u8]),
    {
        tool.begin_interruptible_operation()
            .map_err(ArtifactError::Run)?;
        let input = fs::canonicalize(input).map_err(|error| io_error(input, error))?;
        let extension = input
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "cf" | "cfe" | "epf" | "erf") || !input.is_file() {
            return Err(ArtifactError::UnsupportedFile(input));
        }
        let staging = Staging::create(destination).map_err(super::structural_error)?;
        let snapshot = staging.path().join(format!("input.{extension}"));
        fs::copy(&input, &snapshot).map_err(|error| io_error(&input, error))?;
        let sources = staging.path().join("sources");
        let unpacked = staging.path().join("unpacked");
        fs::create_dir(&unpacked).map_err(|error| io_error(&unpacked, error))?;
        let export = unpacked.join(
            input
                .file_stem()
                .ok_or_else(|| ArtifactError::UnsupportedFile(input.clone()))?,
        );
        let data = staging.path().join("infobase");
        let pid = staging.path().join("ibcmd.pid");
        run(
            tool,
            &pid,
            "create-infobase",
            vec!["infobase".into(), "create".into(), option("--data=", &data)],
            &mut output,
        )?;
        export_sources(tool, &pid, &data, &snapshot, &export, &mut output)?;
        // External exports may replace a dotted filename's suffix with `.xml`.
        // Inspect the actual descriptor instead of predicting the platform's naming rules.
        let (exported, identity) = if let Some(identity) = inspect_identity(&unpacked)? {
            (&unpacked, identity)
        } else {
            let identity = inspect_identity(&export)?
                .ok_or_else(|| ArtifactError::MissingDescriptor(export.clone()))?;
            (&export, identity)
        };
        fs::rename(exported, &sources).map_err(|error| io_error(exported, error))?;
        super::snapshot::capture(&sources)?;
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
        self.staging.path().join("sources")
    }

    /// Publish only into an exclusively claimed new directory; an existing destination is preserved.
    ///
    /// No manifest, Git metadata or existing project source is replaced. The core transaction
    /// removes only the directory it claimed if moving one of the exported entries fails.
    ///
    /// # Errors
    /// Returns unsafe destinations, changed exported identity, I/O or retained rollback failures.
    pub fn publish_new(
        self,
        destination: &Path,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<std::path::PathBuf, ArtifactError> {
        if cancelled() {
            return Err(ArtifactError::Run(crate::build::RunError::Interrupted));
        }
        let destination = new_destination(destination)?;
        let sources = self.sources();
        super::snapshot::capture(&sources)?;
        if inspect_identity(&sources)?.as_ref() != Some(&self.identity) {
            return Err(ArtifactError::SourceChanged(sources));
        }
        eska_project_core::project::create::in_new_directory(&destination, |owned| {
            let check_cancelled = || {
                if cancelled() {
                    Err(
                        eska_project_core::project::create::CreationError::Operation(
                            PublicationError::Cancelled,
                        ),
                    )
                } else {
                    Ok(())
                }
            };
            check_cancelled()?;
            let entries = fs::read_dir(&sources).map_err(publication_io)?;
            for entry in entries {
                check_cancelled()?;
                let entry = entry.map_err(publication_io)?;
                fs::rename(entry.path(), owned.path().join(entry.file_name()))
                    .map_err(publication_io)?;
            }
            check_cancelled()?;
            Ok(destination.clone())
        })
        .map_err(|source| match source {
            eska_project_core::project::create::CreationError::Operation(
                PublicationError::Cancelled,
            ) => ArtifactError::Run(crate::build::RunError::Interrupted),
            source => ArtifactError::Publication {
                path: destination,
                source: Box::new(source),
            },
        })
    }
}

/// Retain filesystem errors separately from the explicit cancellation branch.
const fn publication_io(
    source: std::io::Error,
) -> eska_project_core::project::create::CreationError<PublicationError> {
    eska_project_core::project::create::CreationError::Operation(PublicationError::Io(source))
}

/// Resolve a new XML destination before starting the expensive platform process.
///
/// # Errors
/// Rejects existing destinations, missing parents and paths without a final directory name.
pub fn new_destination(destination: &Path) -> Result<std::path::PathBuf, ArtifactError> {
    let name = destination
        .file_name()
        .ok_or_else(|| ArtifactError::UnsafePath(destination.into()))?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|error| io_error(parent, error))?;
    let destination = parent.join(name);
    match fs::symlink_metadata(&destination) {
        Ok(_) => Err(ArtifactError::UnsafePath(destination)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(destination),
        Err(error) => Err(io_error(&destination, error)),
    }
}

/// Load CF into the isolated database because direct file export rejects some valid CFs.
/// Extensions and external objects retain the platform's direct file export path.
fn export_sources<F>(
    tool: &Ibcmd,
    pid: &Path,
    data: &Path,
    snapshot: &Path,
    destination: &Path,
    output: &mut F,
) -> Result<(), ArtifactError>
where
    F: FnMut(ProcessStream, &[u8]),
{
    let mut arguments = vec![
        "infobase".into(),
        "config".into(),
        "export".into(),
        option("--data=", data),
    ];
    if snapshot
        .extension()
        .is_some_and(|extension| extension == "cf")
    {
        run(
            tool,
            pid,
            "load-configuration",
            vec![
                "infobase".into(),
                "config".into(),
                "load".into(),
                option("--data=", data),
                snapshot.as_os_str().to_owned(),
            ],
            output,
        )?;
    } else {
        arguments.push(option("--file=", snapshot));
    }
    arguments.push(destination.as_os_str().to_owned());
    run(tool, pid, "export", arguments, output)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    /// Owned staging models the result of native export without pretending to test a platform.
    fn prepared(destination: &Path) -> PreparedArtifact {
        let staging = Staging::create(destination).unwrap();
        let sources = staging.path().join("sources");
        fs::create_dir(&sources).unwrap();
        fs::write(sources.join("Configuration.xml"),"<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>Example</Name></Properties></Configuration></MetaDataObject>\r\n").unwrap();
        let identity = inspect_identity(&sources).unwrap().unwrap();
        PreparedArtifact { staging, identity }
    }

    /// Existing directories are never replaced, including an otherwise empty destination.
    #[test]
    fn publication_claims_only_new_destinations() {
        let directory = TestDir::new();
        let destination = directory.0.join("export");
        fs::create_dir(&destination).unwrap();
        assert!(matches!(
            prepared(&destination).publish_new(&destination, &|| false),
            Err(ArtifactError::UnsafePath(_))
        ));
        assert!(destination.is_dir());
        assert!(fs::read_dir(&destination).unwrap().next().is_none());
        fs::remove_dir(&destination).unwrap();
        let source = prepared(&destination);
        let bytes = fs::read(source.sources().join("Configuration.xml")).unwrap();
        assert_eq!(
            source.publish_new(&destination, &|| false).unwrap(),
            destination
        );
        assert_eq!(
            fs::read(destination.join("Configuration.xml")).unwrap(),
            bytes
        );
        assert!(!destination.join("eska.toml").exists());
        assert!(!destination.join(".git").exists());
    }

    /// Cancellation after the first move cleans only the claimed output and preserves sibling bytes.
    #[test]
    fn interrupted_publication_removes_its_partial_output() {
        let directory = TestDir::new();
        let destination = directory.0.join("export");
        fs::write(directory.0.join("keep"), b"user bytes").unwrap();
        let result = prepared(&destination).publish_new(&destination, &|| {
            destination.is_dir() && fs::read_dir(&destination).unwrap().next().is_some()
        });
        assert!(matches!(
            result,
            Err(ArtifactError::Run(crate::build::RunError::Interrupted))
        ));
        assert!(!destination.exists());
        assert_eq!(fs::read(directory.0.join("keep")).unwrap(), b"user bytes");
    }
}
