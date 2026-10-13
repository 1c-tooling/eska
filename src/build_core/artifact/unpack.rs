//! Execute the verified 8.3 pipeline through the existing platform runner.

use std::{fs, path::Path};

use super::export::{export_sources, option, run};
use super::{Identity, UnpackError, io_error, staging::Staging};
use crate::{
    build_core::{Ibcmd, ProcessStream},
    project_core::native::inspect_identity,
};

/// An owned, unpacked artifact whose sources survive until publication or cancellation.
pub struct PreparedArtifact {
    staging: Staging,
    identity: Identity,
    snapshot: super::snapshot::Snapshot,
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
    ) -> Result<Self, UnpackError>
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
            return Err(UnpackError::UnsupportedFile(input));
        }
        let staging = Staging::create(destination)?;
        let snapshot = staging.path.join(format!("input.{extension}"));
        fs::copy(&input, &snapshot).map_err(|error| io_error(&input, error))?;
        let sources = staging.path.join("sources");
        let unpacked = staging.path.join("unpacked");
        fs::create_dir(&unpacked).map_err(|error| io_error(&unpacked, error))?;
        let export = unpacked.join(
            input
                .file_stem()
                .ok_or_else(|| UnpackError::UnsupportedFile(input.clone()))?,
        );
        let data = staging.path.join("infobase");
        let pid = staging.path.join("ibcmd.pid");
        tool.begin_interruptible_operation()
            .map_err(UnpackError::Run)?;
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
                .ok_or_else(|| UnpackError::MissingDescriptor(export.clone()))?;
            (&export, identity)
        };
        fs::rename(exported, &sources).map_err(|error| io_error(exported, error))?;
        let snapshot = super::snapshot::capture(&sources, &|| tool.was_interrupted())?;
        Ok(Self {
            staging,
            identity,
            snapshot,
        })
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

impl PreparedArtifact {
    /// Expose the complete exported file count without handing ownership of staging to a caller.
    #[must_use]
    pub const fn files(&self) -> usize {
        self.snapshot.files
    }

    /// Refuse a modified staging tree instead of publishing bytes outside the prepared operation.
    ///
    /// # Errors
    /// Returns an input change, unsafe entry or filesystem failure.
    pub fn verify(&self) -> Result<(), UnpackError> {
        let sources = self.sources();
        self.verify_snapshot(&sources, &|| false)?;
        if inspect_identity(&sources)?.as_ref() != Some(&self.identity) {
            return Err(UnpackError::SourceChanged(sources));
        }
        Ok(())
    }

    /// Compare copied output to the immutable preparation snapshot, never to fresh mutable inputs.
    pub(super) fn verify_snapshot(
        &self,
        root: &Path,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), UnpackError> {
        if super::snapshot::capture(root, cancelled)? != self.snapshot {
            return Err(UnpackError::SourceChanged(root.to_owned()));
        }
        Ok(())
    }

    /// Publish Designer XML into an exclusively created directory; existing trees are never replaced.
    ///
    /// # Errors
    /// Returns an existing destination, changed staging, unsafe entry or filesystem failure.
    pub fn publish_sources(
        &self,
        destination: &Path,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), UnpackError> {
        super::publication::publish(self, destination, cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    /// Build a private preparation without platform stubs to exercise exact-byte publication.
    fn prepared(root: &Path) -> PreparedArtifact {
        let staging = Staging::create(&root.join("output")).unwrap();
        let sources = staging.path.join("sources");
        fs::create_dir(&sources).unwrap();
        fs::write(sources.join("Processing.xml"), concat!(
            "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'>",
            "<ExternalDataProcessor uuid='12345678-1234-1234-1234-123456789012'>",
            "<Properties><Name>Processing</Name></Properties></ExternalDataProcessor></MetaDataObject>"
        )).unwrap();
        fs::create_dir(sources.join("Empty")).unwrap();
        fs::write(sources.join("Module.bsl"), b"\xef\xbb\xbf// keep\r\n").unwrap();
        let identity = inspect_identity(&sources).unwrap().unwrap();
        let snapshot = super::super::snapshot::capture(&sources, &|| false).unwrap();
        PreparedArtifact {
            staging,
            identity,
            snapshot,
        }
    }

    /// Output survives staging cleanup; exact bytes and empty directories are copied only once.
    #[test]
    fn copies_complete_sources_without_replacing_an_existing_destination() {
        let fixture = TestDir::new();
        let artifact = prepared(&fixture.0);
        let staging = artifact.sources();
        let destination = fixture.0.join("output");
        artifact.publish_sources(&destination, &|| false).unwrap();
        assert!(destination.join("Empty").is_dir());
        assert_eq!(
            fs::read(destination.join("Module.bsl")).unwrap(),
            b"\xef\xbb\xbf// keep\r\n"
        );
        assert!(matches!(
            artifact.publish_sources(&destination, &|| false),
            Err(UnpackError::DestinationExists(_))
        ));
        drop(artifact);
        assert!(!staging.exists());
        assert!(destination.join("Processing.xml").is_file());
    }

    /// A valid identity cannot conceal a changed module or an added unapproved resource.
    #[test]
    fn changed_staging_is_refused_before_claiming_output() {
        for change in ["Module.bsl", "Extra.bin"] {
            let fixture = TestDir::new();
            let artifact = prepared(&fixture.0);
            fs::write(artifact.sources().join(change), b"changed").unwrap();
            let destination = fixture.0.join("output");
            assert!(matches!(
                artifact.publish_sources(&destination, &|| false),
                Err(UnpackError::SourceChanged(_))
            ));
            assert!(!destination.exists());
        }
    }

    /// A new symlink never redirects publication to files outside owned staging.
    #[cfg(unix)]
    #[test]
    fn refuses_a_link_introduced_after_preparation() {
        let fixture = TestDir::new();
        let artifact = prepared(&fixture.0);
        let outside = fixture.0.join("outside");
        fs::write(&outside, "keep").unwrap();
        std::os::unix::fs::symlink(&outside, artifact.sources().join("Linked.bin")).unwrap();
        assert!(matches!(
            artifact.publish_sources(&fixture.0.join("output"), &|| false),
            Err(UnpackError::UnsafePath(_))
        ));
        assert_eq!(fs::read_to_string(outside).unwrap(), "keep");
        assert!(!fixture.0.join("output").exists());
    }

    /// Cancellation after the output claim removes partial copies and retains the prepared source.
    #[test]
    fn cancellation_during_publication_removes_only_claimed_output() {
        let fixture = TestDir::new();
        let artifact = prepared(&fixture.0);
        let destination = fixture.0.join("output");
        let result = artifact.publish_sources(&destination, &|| destination.exists());
        assert!(matches!(
            result,
            Err(UnpackError::Run(crate::build_core::RunError::Interrupted))
        ));
        assert!(!destination.exists());
        artifact.verify().unwrap();
    }
}
