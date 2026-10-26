//! Preserve user ignore rules and add generated-file rules through an exact-byte transaction.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use super::onboarding::{WorkspaceEnrollmentError, replace_exact};

const RULES: &str = include_str!("../../assets/project/.gitignore");

/// A root-owned ignore edit can be rolled back without touching concurrent user changes.
#[derive(Clone, Debug)]
pub(super) struct IgnorePlan {
    path: PathBuf,
    previous: Option<Vec<u8>>,
    updated: Vec<u8>,
}

impl IgnorePlan {
    /// Read only a regular file; append missing rules while retaining BOM, comments and line endings.
    pub fn inspect(
        root: &Path,
        artifacts: Option<&Path>,
    ) -> Result<Self, WorkspaceEnrollmentError> {
        let path = root.join(".gitignore");
        let previous = read(&path)?;
        let text = std::str::from_utf8(previous.as_deref().unwrap_or_default())
            .map_err(|error| failure(&path, io::Error::new(io::ErrorKind::InvalidData, error)))?;
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let mut rules: Vec<String> = RULES
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        if let Some(artifacts) = artifacts {
            rules.push(
                artifact_rule(artifacts)
                    .ok_or_else(|| failure(&path, io::Error::from(io::ErrorKind::InvalidData)))?,
            );
        }
        let mut updated = if previous.is_none() {
            RULES.to_owned()
        } else {
            text.to_owned()
        };
        for rule in rules {
            // A later negation can undo an earlier pattern. Reassert a required rule after it.
            let last_negation = text
                .lines()
                .enumerate()
                .filter_map(|(index, line)| line.starts_with('!').then_some(index))
                .last();
            let exists = text.lines().enumerate().any(|(index, line)| {
                line.trim_start_matches('\u{feff}') == rule
                    && last_negation.is_none_or(|last| index > last)
            });
            if exists || updated[text.len()..].lines().any(|line| line == rule) {
                continue;
            }
            if !updated.is_empty() && !updated.ends_with('\n') {
                updated.push_str(newline);
            }
            updated.push_str(&rule);
            updated.push_str(newline);
        }
        Ok(Self {
            path,
            previous,
            updated: updated.into_bytes(),
        })
    }

    /// Publish only against the bytes observed before the surrounding transaction started.
    pub fn publish(&self) -> Result<(), WorkspaceEnrollmentError> {
        if self.previous.as_deref() == Some(self.updated.as_slice()) {
            return Ok(());
        }
        if let Some(previous) = &self.previous {
            return replace_exact(&self.path, previous, &self.updated);
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.path)
            .map_err(|source| failure(&self.path, source))?;
        if let Err(source) = file.write_all(&self.updated).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(&self.path);
            return Err(failure(&self.path, source));
        }
        Ok(())
    }

    /// Restore exact original bytes, or remove only a newly created and still unchanged file.
    pub fn restore(&self) -> Result<(), WorkspaceEnrollmentError> {
        if self.previous.as_deref() == Some(self.updated.as_slice()) {
            return Ok(());
        }
        if let Some(previous) = &self.previous {
            return replace_exact(&self.path, &self.updated, previous);
        }
        if read(&self.path)?.as_deref() != Some(self.updated.as_slice()) {
            return Err(WorkspaceEnrollmentError::ManifestChanged {
                path: self.path.clone(),
            });
        }
        fs::remove_file(&self.path).map_err(|source| failure(&self.path, source))
    }
}

/// Escape glob metacharacters so a configured artifact directory is matched literally on every OS.
pub(super) fn artifact_rule(path: &Path) -> Option<String> {
    let mut rule = String::new();
    for component in path.components() {
        if component == std::path::Component::CurDir {
            continue;
        }
        rule.push('/');
        for character in component.as_os_str().to_str()?.chars() {
            if matches!(character, '*' | '?' | '[' | ']' | '\\') {
                rule.push('\\');
            }
            rule.push(character);
        }
    }
    rule.push('/');
    Some(rule)
}

/// Do not follow a control-file symlink or replace a directory belonging to the user.
fn read(path: &Path) -> Result<Option<Vec<u8>>, WorkspaceEnrollmentError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => fs::read(path)
            .map(Some)
            .map_err(|source| failure(path, source)),
        Ok(_) => Err(failure(path, io::Error::from(io::ErrorKind::InvalidInput))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(failure(path, source)),
    }
}

/// Keep path coordinates and native errors available to both init and workspace presentation.
fn failure(path: &Path, source: io::Error) -> WorkspaceEnrollmentError {
    WorkspaceEnrollmentError::Io {
        path: path.into(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;
    use std::process::Command;

    /// Real Git matching covers arbitrary export depth, existing rules, caches and generated configs.
    #[test]
    fn ignore_rules_cover_nested_exports_and_restore_exact_user_bytes() {
        let fixture = TestDir::new();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&fixture.0)
                .status()
                .unwrap()
                .success()
        );
        let original = "\u{feff}# пользовательские правила\r\n*.log\r\nConfigDumpInfo.xml\r\n!deep/ConfigDumpInfo.xml\r\n";
        fs::write(fixture.0.join(".gitignore"), original).unwrap();
        let plan = IgnorePlan::inspect(&fixture.0, Some(Path::new("artifacts"))).unwrap();
        plan.publish().unwrap();
        let current = fs::read_to_string(fixture.0.join(".gitignore")).unwrap();
        assert!(current.starts_with(original));
        assert!(!current.replace("\r\n", "").contains('\n'));
        for path in [
            "ConfigDumpInfo.xml",
            "deep/ConfigDumpInfo.xml",
            "src/member/dump/deep/ConfigDumpInfo.xml",
            "src/member/DumpFilesIndex.txt",
            "bsl-analyzer.toml",
            "bsl-analyzer.toml.eska-original-abc",
            ".eska/analyzer-config.lock",
            "src/member/.eska/import/file",
            ".bsl-analyzer/index",
            "artifacts/test.epf",
        ] {
            assert!(
                Command::new("git")
                    .args(["check-ignore", "--no-index", "-q", "--", path])
                    .current_dir(&fixture.0)
                    .status()
                    .unwrap()
                    .success(),
                "{path}"
            );
        }
        for path in [
            "eska.toml",
            "src/Configuration.xml",
            "src/member/Module.bsl",
        ] {
            assert!(
                !Command::new("git")
                    .args(["check-ignore", "--no-index", "-q", "--", path])
                    .current_dir(&fixture.0)
                    .status()
                    .unwrap()
                    .success(),
                "{path}"
            );
        }
        IgnorePlan::inspect(&fixture.0, Some(Path::new("artifacts")))
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(
            fs::read_to_string(fixture.0.join(".gitignore")).unwrap(),
            current
        );
        plan.restore().unwrap();
        assert_eq!(
            fs::read_to_string(fixture.0.join(".gitignore")).unwrap(),
            original
        );
    }
}
