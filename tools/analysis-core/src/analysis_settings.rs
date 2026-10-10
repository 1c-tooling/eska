//! Both tool consumers use the pinned config parser and reject unsupported scopes explicitly.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use eska::tool_cli::{Failure, localization::Locale};
use project_model::ProjectConfig;
use sha2::{Digest, Sha256};

use crate::DiagnosticsConfig;

pub struct Settings {
    pub path: Option<PathBuf>,
    pub sha256: String,
    pub diagnostics: DiagnosticsConfig,
}

/// Read conventional analyzer settings at the selected root, with an optional explicit override.
pub fn load(root: &Path, explicit: Option<&Path>, locale: Locale) -> Result<Settings, Failure> {
    let config = if let Some(path) = explicit {
        let path = root.join(path);
        ProjectConfig::load_from_file(&path)
            .map_err(|_| Failure::new("analysis_config_invalid").at(path.to_string_lossy()))?
    } else {
        ProjectConfig::load(root)
            .map_err(|_| Failure::new("analysis_config_invalid").at(root.to_string_lossy()))?
            .unwrap_or_default()
    };
    if !config.source_exclude.is_empty()
        || config
            .extensions
            .as_ref()
            .is_some_and(|entries| !entries.is_empty())
        || config
            .externals
            .as_ref()
            .is_some_and(|entries| !entries.is_empty())
        || config.configuration_root.is_some()
        || config.configuration_dependency.is_some()
        || config.analysis.diff_base.is_some()
        || !config.analysis.ignored_authors.is_empty()
        || config.diagnostics.baseline.is_some()
    {
        return Err(Failure::new("analysis_scope_unsupported"));
    }
    let rules = config.diagnostics.rules_json();
    let bytes = serde_json::to_vec(&serde_json::json!({"profile":"syntax", "diagnostics":rules}))
        .map_err(|_| Failure::new("analysis_config_invalid"))?;
    let mut diagnostics: DiagnosticsConfig =
        serde_json::from_value(rules).map_err(|_| Failure::new("analysis_config_invalid"))?;
    diagnostics.locale = match locale {
        Locale::RuRu => crate::Locale::Ru,
        Locale::EnUs => crate::Locale::En,
    };
    let sha256 = Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hash, byte| {
            let _ = write!(hash, "{byte:02x}");
            hash
        });
    if let Some(path) = config.config_file_path() {
        eska::tool_cli::path_text(path)?;
    }
    Ok(Settings {
        path: config.config_file_path().map(Path::to_owned),
        sha256,
        diagnostics,
    })
}
