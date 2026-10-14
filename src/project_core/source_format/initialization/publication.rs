//! Attachment owns new service entries and exact attribute additions, never the user root or source.

use std::{fs, path::Path};

use sha2::{Digest, Sha256};

use crate::project_core::{
    creation::ImportedProject,
    publish::{self, PublishError},
};

use super::{
    ACCEPTED, Error, PROJECT_SOURCE_IMPORT_DIRECTORY, ProjectInitializationPlan, checked_directory,
    config, files, optional_inventory,
    staging::{CandidateStore, Candidates, Stage},
};

#[cfg(test)]
mod tests;

struct Owned {
    service_parent: bool,
    directories: Vec<CandidateStore>,
    ignore: Option<String>,
    attributes: bool,
}

impl ProjectInitializationPlan {
    /// Publish only new project control files, preserving selected authoring and existing evidence.
    ///
    /// Settings are published last. Ordinary failure rolls back only unchanged entries created by
    /// this invocation and its exact attributes addition. Intervening edits remain in place and
    /// produce `source_cleanup_failed`; process termination is not a filesystem-wide transaction.
    ///
    /// # Errors
    /// Returns stale input, concurrent output, unsafe storage, candidate creation or cleanup failures.
    pub fn publish(&self) -> Result<ImportedProject, Error> {
        self.recheck()?;
        let stage = Stage::claim(self)?;
        let mut owned = Owned {
            service_parent: false,
            directories: Vec::new(),
            ignore: None,
            attributes: false,
        };
        let result = stage
            .prepare(self)
            .and_then(|candidates| self.populate(candidates, &mut owned));
        let result = match result {
            Err(error) => owned.rollback(self, error),
            Ok(project) => Ok(project),
        };
        stage.remove().map_err(|error| {
            error.in_field(
                result
                    .as_ref()
                    .err()
                    .map_or("source_project_created", |error| error.code),
            )
        })?;
        result
    }

    /// Create candidate evidence and transport before exposing the initialized project manifest.
    fn populate(
        &self,
        candidates: Candidates,
        owned: &mut Owned,
    ) -> Result<ImportedProject, Error> {
        owned.directories = candidates.stores;
        self.publish_candidates(owned)?;
        self.finish_controls(owned, candidates.accepted, candidates.ancestor)
    }

    /// Candidate fingerprints precede exclusive publication into the independently existing root.
    fn publish_candidates(&self, owned: &mut Owned) -> Result<(), Error> {
        self.recheck()?;
        let service = self.root.join(".eska");
        match fs::symlink_metadata(&service) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&service).map_err(|error| files::io(&error))?;
                owned.service_parent = true;
            }
            Err(error) => return Err(files::io(&error)),
            Ok(_) => checked_directory(&service)?,
        }
        for store in &mut owned.directories {
            super::check_service_parent(&self.root)?;
            store.publish(self)?;
        }
        self.check_evidence(owned)?;
        Ok(())
    }

    /// Transport and the last manifest publication proceed only while precomputed evidence matches.
    fn finish_controls(
        &self,
        owned: &mut Owned,
        accepted: super::AcceptedRef,
        ancestor: super::ImportAncestorRef,
    ) -> Result<ImportedProject, Error> {
        self.check_evidence(owned)?;
        let ignore_path = self.root.join(".gitignore");
        match fs::symlink_metadata(&ignore_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let text = "/build/\n/.eska/generations/\n".to_owned();
                publish::publish_new(&self.root, Path::new(".gitignore"), &text)
                    .map_err(|error| publication_error(&error))?;
                owned.ignore = Some(text);
            }
            Err(error) => return Err(files::io(&error)),
            Ok(_) => (),
        }
        self.transport.publish()?;
        owned.attributes = !self.transport.is_empty();
        self.check_evidence(owned)?;
        publish::publish_new(&self.root, Path::new(config::FILE_NAME), &self.manifest)
            .map_err(|error| publication_error(&error))?;
        Ok(ImportedProject {
            project: self.project.clone(),
            accepted,
            initial_ancestor: Some(ancestor),
            manifest_sha256: files::hex(&Sha256::digest(self.manifest.as_bytes())),
        })
    }

    /// Both old evidence and newly owned candidates are bound before manifest publication.
    fn check_evidence(&self, owned: &Owned) -> Result<(), Error> {
        self.recheck_source()?;
        for (path, original) in [
            (ACCEPTED, &self.accepted),
            (PROJECT_SOURCE_IMPORT_DIRECTORY, &self.history),
        ] {
            let expected = original.as_ref().or_else(|| {
                owned
                    .directories
                    .iter()
                    .find(|store| store.relative == path && store.claimed)
                    .map(|store| &store.inventory)
            });
            if optional_inventory(&self.root.join(path))?.as_ref() != expected {
                return Err(Error::root("source_input_changed"));
            }
        }
        Ok(())
    }
}

impl Owned {
    /// An entry edited after creation is retained instead of being deleted as transient state.
    fn rollback(
        self,
        plan: &ProjectInitializationPlan,
        original: Error,
    ) -> Result<ImportedProject, Error> {
        if super::check_root(&plan.root).is_err() {
            return Err(Error::root("source_cleanup_failed")
                .in_field(original.code)
                .in_field(plan.root.to_string_lossy().into_owned()));
        }
        let mut remaining = Vec::new();
        if self.attributes {
            let restored = plan.transport.original().map_or_else(
                || remove_exact_file(&plan.root.join(".gitattributes"), plan.transport.updated()),
                |text| {
                    publish::publish_snapshot(
                        &plan.root,
                        Path::new(".gitattributes"),
                        plan.transport.updated(),
                        text,
                    )
                    .is_ok()
                },
            );
            if !restored {
                remaining.push(".gitattributes");
            }
        }
        if let Some(text) = &self.ignore
            && !remove_exact_file(&plan.root.join(".gitignore"), text)
        {
            remaining.push(".gitignore");
        }
        for store in self
            .directories
            .into_iter()
            .rev()
            .filter(|store| store.claimed)
        {
            let directory = plan.root.join(store.relative);
            if super::check_service_parent(&plan.root).is_err()
                || !store.matches_owned_partial(plan)
                || fs::remove_dir_all(&directory).is_err()
            {
                remaining.push(store.relative);
            }
        }
        if self.service_parent && fs::remove_dir(plan.root.join(".eska")).is_err() {
            remaining.push(".eska");
        }
        if remaining.is_empty() {
            Err(original)
        } else {
            Err(Error::root("source_cleanup_failed")
                .in_field(original.code)
                .in_field(remaining.join(",")))
        }
    }
}

/// Removing a new ordinary file requires its original bytes; aliases and later edits remain intact.
fn remove_exact_file(path: &Path, expected: &str) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
        && fs::read(path).is_ok_and(|bytes| bytes == expected.as_bytes())
        && fs::remove_file(path).is_ok()
}

/// Publication errors remain machine-readable and receive localized presentation at the boundary.
fn publication_error(error: &PublishError) -> Error {
    Error::root(match error {
        PublishError::Conflict => "source_edit_conflict",
        PublishError::UnsafePath => "source_entry_unsupported",
        PublishError::ReadOnly => "source_edit_read_only",
        PublishError::Io(_) => "source_io_error",
    })
}
