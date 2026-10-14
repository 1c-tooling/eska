//! Native unpacking uses owned ignored staging; failures never recursively remove a project root.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::build_core::artifact::{
    UnpackError,
    snapshot::{self, Snapshot},
    staging::Staging,
};

/// Preserve by default: only an explicitly checked successful operation may clear this native input.
pub(super) struct NativeStage {
    owned: Staging,
    expected: Option<Snapshot>,
}

impl NativeStage {
    /// Claim a unique service child, never the final destination or a shared service directory.
    pub(super) fn claim(destination: &Path) -> Result<Self, UnpackError> {
        let mut owned = Staging::create(destination)?;
        owned.preserve = true;
        Ok(Self {
            owned,
            expected: None,
        })
    }

    /// The build companion must claim this absent child before publishing complete native XML.
    pub(super) fn native(&self) -> PathBuf {
        self.owned.path.join("native")
    }

    /// Keep a retained path available when a child or parent onboarding cannot complete.
    pub(super) fn root(&self) -> &Path {
        &self.owned.path
    }

    /// Seal the complete native result before the project tool can consume it read-only.
    pub(super) fn seal(&mut self) -> Result<(), UnpackError> {
        self.expected = Some(snapshot::capture(&self.owned.path, &|| false)?);
        Ok(())
    }

    /// Intervening staged edits or unknown entries preserve all staged data for diagnosis.
    ///
    /// This storage is the invocation's disposable native input, separate from the project
    /// published by `eska-project`; no failure path calls this method for that project root.
    pub(super) fn cleanup(self) -> Result<(), UnpackError> {
        let current = snapshot::capture(&self.owned.path, &|| false)?;
        if self.expected.as_ref() != Some(&current) {
            return Err(UnpackError::SourceChanged(self.owned.path.clone()));
        }
        fs::remove_dir_all(&self.owned.path).map_err(|source| UnpackError::Io {
            path: self.owned.path.clone(),
            source,
        })
    }
}
