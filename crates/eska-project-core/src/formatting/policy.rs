//! Strict manifests resolve into a complete versioned style without starting any tool.

use super::{Error, KeywordCase, Options, Patch};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

mod resolve;
pub(super) use resolve::legacy;
pub use resolve::{Layer, resolve};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Bsl,
    Sdbl,
}

/// A named profile inherits styles, never source roots or installation settings.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
    #[serde(skip_serializing_if = "Patch::is_empty")]
    pub common: Patch,
    #[serde(skip_serializing_if = "Patch::is_empty")]
    pub bsl: Patch,
    #[serde(skip_serializing_if = "Patch::is_empty")]
    pub sdbl: Patch,
}

/// Ordered path rules are relative to the manifest that declared them.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Override {
    pub include: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    #[serde(default, skip_serializing_if = "Patch::is_empty")]
    pub common: Patch,
    #[serde(default, skip_serializing_if = "Patch::is_empty")]
    pub bsl: Patch,
    #[serde(default, skip_serializing_if = "Patch::is_empty")]
    pub sdbl: Patch,
}

/// Absence is kept distinct from an explicit built-in profile and empty exclusions.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields, try_from = "UncheckedConfig")]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, Profile>,
    #[serde(skip_serializing_if = "Patch::is_empty")]
    pub common: Patch,
    #[serde(skip_serializing_if = "Patch::is_empty")]
    pub bsl: Patch,
    #[serde(skip_serializing_if = "Patch::is_empty")]
    pub sdbl: Patch,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<Override>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UncheckedConfig {
    profile: Option<String>,
    profile_version: Option<u32>,
    exclude: Option<Vec<String>>,
    profiles: BTreeMap<String, Profile>,
    common: Patch,
    bsl: Patch,
    sdbl: Patch,
    overrides: Vec<Override>,
}

impl TryFrom<UncheckedConfig> for Config {
    type Error = Error;

    /// Reject unsupported scalar values while the ordinary strict manifest loader runs.
    fn try_from(raw: UncheckedConfig) -> Result<Self, Error> {
        let config = Self {
            profile: raw.profile,
            profile_version: raw.profile_version,
            exclude: raw.exclude,
            profiles: raw.profiles,
            common: raw.common,
            bsl: raw.bsl,
            sdbl: raw.sdbl,
            overrides: raw.overrides,
        };
        config.validate()?;
        Ok(config)
    }
}

impl Config {
    /// Preserve compact existing manifests when no formatting policy was configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Validate values and patterns without requiring a member's inherited profile definitions.
    ///
    /// # Errors
    /// Rejects unknown profile revisions, incompatible options and unsafe patterns.
    pub fn validate(&self) -> Result<(), Error> {
        if self.profile_version.is_some_and(|value| value != 1) {
            return Err(Error::new("unsupported_profile_version", "profile_version"));
        }
        validate_profile(&Profile {
            common: self.common.clone(),
            bsl: self.bsl.clone(),
            sdbl: self.sdbl.clone(),
            based_on: None,
        })?;
        for (name, profile) in &self.profiles {
            if name == "1c" || name.is_empty() || name.len() > 128 {
                return Err(Error::new("invalid_profile", name));
            }
            validate_profile(profile)?;
        }
        validate_patterns(self.exclude.as_deref().unwrap_or_default())?;
        if self.overrides.len() > 256 || self.profiles.len() > 128 {
            return Err(Error::new("policy_too_large", "format"));
        }
        for rule in &self.overrides {
            if rule.include.is_empty() {
                return Err(Error::new("invalid_pattern", "include"));
            }
            validate_patterns(&rule.include)?;
            validate_patterns(&rule.exclude)?;
            validate_profile(&Profile {
                common: rule.common.clone(),
                bsl: rule.bsl.clone(),
                sdbl: rule.sdbl.clone(),
                based_on: None,
            })?;
        }
        Ok(())
    }
}

/// Check options in their actual language scopes, including explicit but inapplicable keys.
fn validate_profile(profile: &Profile) -> Result<(), Error> {
    validate_patch(&profile.common, None)?;
    validate_patch(&profile.bsl, Some(Language::Bsl))?;
    validate_patch(&profile.sdbl, Some(Language::Sdbl))
}

/// Known options for another language fail instead of becoming silently ignored configuration.
pub(super) fn validate_patch(patch: &Patch, language: Option<Language>) -> Result<(), Error> {
    let bsl_only = patch.align_assignments.is_some() || patch.blank_lines_between_methods.is_some();
    let sdbl_only = patch.select_items.is_some()
        || patch.clause_newlines.is_some()
        || patch.align_aliases.is_some()
        || patch.condition_items.is_some()
        || patch.group_items.is_some()
        || patch.order_items.is_some()
        || patch.blank_lines_between_queries.is_some();
    if (bsl_only && language != Some(Language::Bsl))
        || (sdbl_only && language != Some(Language::Sdbl))
    {
        return Err(Error::new("unsupported_option_scope", "format"));
    }
    let mut options = Options::default();
    patch.apply(&mut options, &mut BTreeMap::new(), "validation");
    options.validate_values()
}

/// Limit the portable glob subset rather than interpreting filesystem or shell expressions.
fn validate_patterns(patterns: &[String]) -> Result<(), Error> {
    if patterns.len() > 256 {
        return Err(Error::new("policy_too_large", "patterns"));
    }
    for value in patterns {
        if value.is_empty()
            || value.len() > 1024
            || value.starts_with('/')
            || value.contains(['\\', ':', '[', ']', '\0'])
            || value.split('/').any(|part| part == ".." || part == ".")
        {
            return Err(Error::new("invalid_pattern", value));
        }
    }
    Ok(())
}

/// Effective policies carry provenance and exclusions for CLI, editor and CI parity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Effective {
    pub schema_version: u32,
    pub profile: String,
    pub profile_version: u32,
    pub language: Language,
    pub options: Options,
    pub origins: BTreeMap<String, String>,
    pub excluded: bool,
}

/// Match *, ?, and ** with bounded dynamic programming; **/ also matches no directories.
#[must_use]
pub fn matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<_> = pattern.chars().collect();
    let path: Vec<_> = path.chars().collect();
    if pattern.len() > 1024 || path.len() > 32_768 {
        return false;
    }
    let mut table = vec![vec![false; path.len() + 1]; pattern.len() + 1];
    table[pattern.len()][path.len()] = true;
    for at in (0..pattern.len()).rev() {
        for offset in (0..=path.len()).rev() {
            table[at][offset] = match pattern[at] {
                '*' => {
                    table[at + 1][offset]
                        || (offset < path.len()
                            && (path[offset] != '/'
                                || pattern.get(at + 1) == Some(&'*')
                                || at > 0 && pattern[at - 1] == '*')
                            && table[at][offset + 1])
                        || (pattern.get(at..at + 3) == Some(&['*', '*', '/'])
                            && table[at + 3][offset])
                }
                '?' => offset < path.len() && path[offset] != '/' && table[at + 1][offset + 1],
                value => offset < path.len() && value == path[offset] && table[at + 1][offset + 1],
            };
        }
    }
    table[0][0]
}

/// Convert native separators only after checking the source is under this declaring manifest.
pub(super) fn relative_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .and_then(Path::to_str)
        .map(|value| value.replace('\\', "/"))
}

/// The built-in query profile differs only in language-specific token case and layout.
pub(super) fn defaults(language: Language) -> Options {
    Options {
        keyword_case: if language == Language::Sdbl {
            KeywordCase::Upper
        } else {
            KeywordCase::Canonical
        },
        ..Options::default()
    }
}
