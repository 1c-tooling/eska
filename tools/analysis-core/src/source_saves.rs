//! Saving validates the exact current editor input without replacing its authority or revision.

use eska::{
    analysis_core::{InputOrigin, InputState},
    project_core::source_format::SourcePath,
};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use crate::{AnalysisSession, ModuleState, QueryTextState, SessionRequestError, SessionRevision};

/// Only declared authoring text belongs to the guarded save contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceSaveKind {
    Yaml,
    Bsl,
    Sdbl,
}

/// Rejected save input never changes editor bytes, kernel inputs or source quota.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceSaveError {
    Request(SessionRequestError),
    UnsupportedSource,
    InputUnavailable,
    DocumentNotOpen,
    DocumentVersion { current: i64, received: i64 },
    SourceConflict,
    InvalidHash,
    UnsupportedBom,
}

/// The candidate retains raw UTF-8 bytes while its fingerprint names the logical editor text.
#[derive(Debug)]
pub struct SourceSaveInput<'a> {
    source: SourcePath,
    kind: SourceSaveKind,
    revision: SessionRevision,
    document_version: i64,
    draft_sha256: String,
    disk_sha256: String,
    text: &'a str,
}

impl AnalysisSession {
    /// Bind a complete candidate to an open selected input before opening accepted policy storage.
    ///
    /// Save admission is independent of semantic scope. A UTF-8 BOM belongs to BSL/SDBL disk
    /// bytes, while their journal text omits that one encoding marker. YAML rejects a BOM.
    ///
    /// # Errors
    /// Returns stale/unknown input, unavailable bytes, a closed/wrong-version document or a hash mismatch.
    pub fn source_save_input<'a>(
        &self,
        source: &SourcePath,
        expected: SessionRevision,
        document_version: i64,
        draft_sha256: &str,
        text: &'a str,
    ) -> Result<SourceSaveInput<'a>, SourceSaveError> {
        self.validate_revision(expected)
            .map_err(SourceSaveError::Request)?;
        if !valid_sha256(draft_sha256) {
            return Err(SourceSaveError::InvalidHash);
        }
        let (kind, origin, bytes, fingerprint) = if self.owner(source).is_some() {
            let view = self
                .metadata(source, expected)
                .map_err(SourceSaveError::Request)?;
            let InputState::Ready(snapshot) = view.input else {
                return Err(SourceSaveError::InputUnavailable);
            };
            (
                SourceSaveKind::Yaml,
                view.origin,
                snapshot.source_bytes,
                view.source_sha256,
            )
        } else if let Some(input) = self.module(source) {
            let ModuleState::Ready { bytes } = input.state else {
                return Err(SourceSaveError::InputUnavailable);
            };
            (
                SourceSaveKind::Bsl,
                input.origin,
                bytes,
                input.source_sha256.clone(),
            )
        } else if let Some(input) = self.query_input(source) {
            let QueryTextState::Ready(current) = &input.state else {
                return Err(SourceSaveError::InputUnavailable);
            };
            (
                SourceSaveKind::Sdbl,
                input.origin,
                current.len() as u64,
                input.source_sha256.clone(),
            )
        } else {
            return Err(SourceSaveError::UnsupportedSource);
        };
        let InputOrigin::Overlay {
            document_version: current,
        } = origin
        else {
            return Err(SourceSaveError::DocumentNotOpen);
        };
        if current != document_version {
            return Err(SourceSaveError::DocumentVersion {
                current,
                received: document_version,
            });
        }
        let logical = match kind {
            SourceSaveKind::Yaml if text.starts_with('\u{feff}') => {
                return Err(SourceSaveError::UnsupportedBom);
            }
            SourceSaveKind::Yaml => text,
            SourceSaveKind::Bsl | SourceSaveKind::Sdbl => {
                text.strip_prefix('\u{feff}').unwrap_or(text)
            }
        };
        if u64::try_from(logical.len()).ok() != Some(bytes)
            || fingerprint.as_deref() != Some(draft_sha256)
            || sha256(logical) != draft_sha256
        {
            return Err(SourceSaveError::SourceConflict);
        }
        Ok(SourceSaveInput {
            source: source.clone(),
            kind,
            revision: expected,
            document_version,
            draft_sha256: draft_sha256.to_owned(),
            disk_sha256: sha256(text),
            text,
        })
    }
}

impl SourceSaveInput<'_> {
    /// The selected declaration, rather than a URI or editable UUID, locates the destination.
    #[must_use]
    pub const fn source(&self) -> &SourcePath {
        &self.source
    }

    /// The declared input kind selects the existing core publication planner.
    #[must_use]
    pub const fn kind(&self) -> SourceSaveKind {
        self.kind
    }

    /// Save leaves the accepted journal revision unchanged.
    #[must_use]
    pub const fn revision(&self) -> SessionRevision {
        self.revision
    }

    /// Save cannot acknowledge another editor version.
    #[must_use]
    pub const fn document_version(&self) -> i64 {
        self.document_version
    }

    /// Logical text identity is independent of the optional BSL/SDBL encoding marker.
    #[must_use]
    pub fn draft_sha256(&self) -> &str {
        &self.draft_sha256
    }

    /// The result fingerprint includes every raw byte that publication will store.
    #[must_use]
    pub fn disk_sha256(&self) -> &str {
        &self.disk_sha256
    }

    /// Publication stores the exact supplied candidate, including BOM and line endings.
    #[must_use]
    pub const fn text(&self) -> &str {
        self.text
    }
}

/// Protocol fingerprints have one canonical lowercase spelling.
pub fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Core and transport compare exact UTF-8 bytes without locale-dependent formatting.
pub fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut hash, byte| {
            let _ = write!(hash, "{byte:02x}");
            hash
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AnalysisScope, SessionOptions,
        session::tests::{owner, support::TestDirectory},
    };
    use eska::{analysis_core::InputRequest, project_core::source_format::SourceReader};
    use std::fs;

    const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
    const OWNER: &str = "CommonModules/Tools/object.yaml";
    const LOGICAL: &str = "// Я😀 draft\r\nProcedure Draft(\r\nEndProcedure\r\n";

    /// Local syntax storage has the same save input authority without any semantic admission.
    fn project() -> (TestDirectory, std::path::PathBuf, AnalysisSession) {
        let directory = TestDirectory::new();
        let root = crate::test_project::project(&directory.0, "// saved\n", false);
        let source = root.join("src");
        let session = AnalysisSession::from_reader_with_scope(
            SourceReader::open(&source).unwrap(),
            SessionOptions::default(),
            AnalysisScope::LocalSyntax,
        )
        .unwrap();
        (directory, source, session)
    }

    /// Raw encoding markers do not alter the logical current draft or its exact version and hash.
    #[test]
    fn source_save_input_keeps_overlay_revision_and_logical_bom_boundary() {
        let (_directory, source, mut session) = project();
        let path = owner(MODULE);
        let request = InputRequest::default();
        session
            .apply_module_overlay(&path, session.revision(), 9, LOGICAL, &request)
            .unwrap();
        let before = session.summary();
        let raw = format!("\u{feff}{LOGICAL}");
        let input = session
            .source_save_input(&path, session.revision(), 9, &sha256(LOGICAL), &raw)
            .unwrap();
        assert_eq!(input.kind(), SourceSaveKind::Bsl);
        assert_eq!(input.revision(), session.revision());
        assert_eq!(input.document_version(), 9);
        assert_eq!(input.draft_sha256(), sha256(LOGICAL));
        assert_eq!(input.disk_sha256(), sha256(&raw));
        assert_eq!(input.text(), raw);
        assert_eq!(session.summary(), before);
        assert_eq!(
            fs::read_to_string(source.join(MODULE)).unwrap(),
            "// saved\n"
        );
        for (version, fingerprint, text) in [
            (8, sha256(LOGICAL), raw.as_str()),
            (9, "0".repeat(64), raw.as_str()),
            (9, sha256(LOGICAL), "// different"),
        ] {
            assert!(
                session
                    .source_save_input(&path, session.revision(), version, &fingerprint, text)
                    .is_err()
            );
        }
        assert_eq!(session.summary(), before);
    }

    /// An invalid owner cannot use older saved bytes, while a newer repaired draft regains normal admission.
    #[test]
    fn source_save_input_preserves_yaml_failure_repair_and_bom_refusal() {
        let (_directory, source, mut session) = project();
        let path = owner(OWNER);
        let saved = fs::read_to_string(source.join(OWNER)).unwrap();
        let request = InputRequest::default();
        session
            .apply_overlay(&path, session.revision(), 1, "broken: [", &request)
            .unwrap();
        assert!(matches!(
            session.source_save_input(
                &path,
                session.revision(),
                1,
                &sha256("broken: ["),
                "broken: ["
            ),
            Err(SourceSaveError::InputUnavailable)
        ));
        session
            .apply_overlay(&path, session.revision(), 2, &saved, &request)
            .unwrap();
        assert_eq!(
            session
                .source_save_input(&path, session.revision(), 2, &sha256(&saved), &saved)
                .unwrap()
                .kind(),
            SourceSaveKind::Yaml
        );
        assert!(matches!(
            session.source_save_input(
                &path,
                session.revision(),
                2,
                &sha256(&saved),
                &format!("\u{feff}{saved}")
            ),
            Err(SourceSaveError::UnsupportedBom)
        ));
        assert_eq!(fs::read_to_string(source.join(OWNER)).unwrap(), saved);
    }
}
