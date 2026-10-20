//! Portable patch descriptions; Git snapshots and native execution stay with their tool owners.

use serde::{Deserialize, Serialize};

/// The existing schema-1 public patch plan, independent of a repository implementation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PatchDescription {
    pub base: String,
    pub base_commit: String,
    pub merge_base: String,
    pub head: String,
    pub name: String,
    pub modules: Vec<PatchModule>,
    pub ignored_paths: Vec<String>,
    pub changes: Vec<PatchChange>,
}

impl PatchDescription {
    /// Reject unsafe names and inconsistent commit identities before consuming a process reply.
    ///
    /// # Errors
    /// Returns the malformed field as a path/descriptor failure, without interpreting it as a path.
    pub fn validate(&self) -> Result<(), PatchError> {
        for id in [&self.base_commit, &self.merge_base, &self.head] {
            if id.len() != 40 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(PatchError::new("descriptor", id));
            }
        }
        if self.base.is_empty() || self.name != format!("EskaPatch_{}", &self.head[..12]) {
            return Err(PatchError::new("path", &self.name));
        }
        let mut names = std::collections::HashSet::new();
        for module in &self.modules {
            if !valid_module_name(&module.name)
                || !names.insert(&module.name)
                || module.methods.is_empty()
                || module.methods.iter().any(String::is_empty)
            {
                return Err(PatchError::new("path", &module.name));
            }
        }
        Ok(())
    }
}

/// A common-module identifier must remain one filename on every supported filesystem.
#[must_use]
pub fn valid_module_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_alphabetic() || character == '_')
        && characters.all(|character| character.is_alphanumeric() || character == '_')
}

/// Generated patch text keeps exactly one UTF-8 BOM and the established CRLF representation.
#[must_use]
pub fn normalize_designer_text(text: &str) -> String {
    format!(
        "\u{feff}{}",
        text.trim_start_matches('\u{feff}')
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
    )
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PatchModule {
    pub name: String,
    pub methods: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PatchChange {
    pub path: String,
    pub decision: String,
    pub reason: String,
}

/// Keep machine classification portable; each CLI localizes its presentation.
#[derive(Debug, Serialize)]
pub struct PatchError {
    pub code: String,
    pub detail: String,
}

impl PatchError {
    /// Retain complete failure details without importing Git, native tools or localization.
    pub fn new(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            detail: detail.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Model a valid committed projection without requiring Git or a native process.
    fn description() -> PatchDescription {
        PatchDescription {
            base: "main".into(),
            base_commit: "a".repeat(40),
            merge_base: "a".repeat(40),
            head: "b".repeat(40),
            name: "EskaPatch_bbbbbbbbbbbb".into(),
            modules: vec![PatchModule {
                name: "Math".into(),
                methods: vec!["AddOne".into()],
            }],
            ignored_paths: vec![],
            changes: vec![],
        }
    }

    /// A process reply cannot redirect generated modules or trigger a UTF-8 slice panic.
    #[test]
    fn rejects_escaping_names_duplicate_modules_and_incomplete_commit_ids() {
        description().validate().unwrap();
        for name in ["../user", "a/b", "a\\b", "C:user", ""] {
            let mut plan = description();
            plan.modules[0].name = name.into();
            assert_eq!(plan.validate().unwrap_err().code, "path");
        }
        for head in ["short".into(), "Ж".repeat(20)] {
            let mut plan = description();
            plan.head = head;
            assert_eq!(plan.validate().unwrap_err().code, "descriptor");
        }
        let mut plan = description();
        plan.modules.push(plan.modules[0].clone());
        assert_eq!(plan.validate().unwrap_err().code, "path");
        let mut plan = description();
        plan.name = "../output".into();
        assert_eq!(plan.validate().unwrap_err().code, "path");
    }
}
