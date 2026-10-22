//! Related property descriptors reuse the recoverable metadata journal without rename movements.

use super::{
    Guard, Journal, MAX_JOURNAL_BYTES, MAX_XML_BYTES, StoredFile, cleanup_preparation, io,
    read_text,
};
use crate::project::metadata_edit::{EditError, PropertyFileEdit, snapshot};
use std::collections::BTreeSet;
use std::{fs, path::Path};

impl Guard {
    /// Verify the dependency graph before and after publishing the backend-generated file set.
    pub fn publish_properties(
        &self,
        source: &Path,
        files: &[&PropertyFileEdit],
        before: impl FnOnce() -> Result<(), EditError>,
        after: impl FnOnce() -> Result<(), EditError>,
    ) -> Result<(), EditError> {
        self.check_clear(source)?;
        let directory = io::path(&self.directory, Path::new("rename"))?;
        fs::create_dir(&directory).map_err(EditError::Io)?;
        let prepared = Self::prepare_properties(source, files, &directory)
            .and_then(|journal| before().map(|()| journal));
        let journal = match prepared {
            Ok(journal) => journal,
            Err(error) => {
                // No source has changed; only files owned by this staging format can be removed.
                let _ = cleanup_preparation(&directory);
                return Err(error);
            }
        };
        Self::commit_checked(&journal, &directory, after)
    }

    /// Exact original bytes are verified even for no-op members that constrain the operation.
    fn prepare_properties(
        source: &Path,
        files: &[&PropertyFileEdit],
        directory: &Path,
    ) -> Result<Journal, EditError> {
        if source.canonicalize().map_err(EditError::Io)? != source {
            return Err(EditError::UnsafePath);
        }
        let mut journal = Journal {
            schema_version: 1,
            source: source.to_owned(),
            files: Vec::new(),
            moves: Vec::new(),
        };
        let mut seen = BTreeSet::new();
        for file in files {
            if !seen.insert(&file.path) {
                return Err(EditError::UnsupportedValue);
            }
            let path = io::path(source, &file.path)?;
            let before = read_text(&path, MAX_XML_BYTES).map_err(EditError::Io)?;
            if before != file.plan.original() {
                return Err(EditError::Conflict);
            }
            if file.plan.is_empty() {
                continue;
            }
            if fs::metadata(&path)
                .map_err(EditError::Io)?
                .permissions()
                .readonly()
            {
                return Err(EditError::ReadOnly);
            }
            journal.stage_file(directory, &file.path, &before, file.plan.output())?;
        }
        journal.make_ready(directory)?;
        Ok(journal)
    }
}

impl Journal {
    /// Sync both directions before recording a source file as part of the journal.
    pub(super) fn stage_file(
        &mut self,
        directory: &Path,
        path: &Path,
        before: &str,
        after: &str,
    ) -> Result<(), EditError> {
        let index = self.files.len();
        io::create(
            &directory.join(format!("{index}.before")),
            before.as_bytes(),
        )?;
        io::create(&directory.join(format!("{index}.after")), after.as_bytes())?;
        self.files.push(StoredFile {
            path: path.to_owned(),
            before: snapshot(before),
            after: snapshot(after),
        });
        Ok(())
    }

    /// Recovery may publish or restore a journal only after its complete manifest is synced.
    pub(super) fn make_ready(&self, directory: &Path) -> Result<(), EditError> {
        let bytes = serde_json::to_vec(self).map_err(|_| EditError::InvalidValue)?;
        if bytes.len() as u64 > MAX_JOURNAL_BYTES {
            return Err(EditError::UnsupportedValue);
        }
        io::create(&directory.join("journal.json"), &bytes)?;
        io::create(&directory.join("ready"), b"1")
    }
}
