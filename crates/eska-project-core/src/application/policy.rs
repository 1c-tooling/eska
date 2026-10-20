//! Analyzer policy is preserved as canonical data; executable schema validation stays external.

use serde::{Deserialize, Serialize, Serializer, ser::Error as _};

use super::SettingsError;

/// Canonical TOML preserves arbitrary upstream policy values, including floats, without a runtime.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct AnalyzerPolicy(String);

impl AnalyzerPolicy {
    /// Validate ownership boundaries without copying the analyzer's evolving diagnostics schema.
    ///
    /// # Errors
    /// Rejects paths, composition or platform settings owned by the ESKA project context.
    pub fn parse(table: &toml::Table) -> Result<Self, SettingsError> {
        for key in ["target_platform_version", "compatibility_mode"] {
            if table.contains_key(key) {
                return Err(SettingsError::ManagedAnalyzerField { field: key.into() });
            }
        }
        if let Some(value) = table.get("source") {
            let source = value
                .as_table()
                .ok_or(SettingsError::InvalidAnalyzerPolicy)?;
            for key in ["root", "extensions", "externals", "configuration"] {
                if source.contains_key(key) {
                    return Err(SettingsError::ManagedAnalyzerField {
                        field: format!("source.{key}"),
                    });
                }
            }
        }
        toml::to_string(table)
            .map(Self)
            .map_err(|_| SettingsError::InvalidAnalyzerPolicy)
    }

    /// Return deterministic policy text for external validation and generation.
    #[must_use]
    pub fn as_toml(&self) -> &str {
        &self.0
    }

    /// An empty policy adds no tools table when serializing a manifest.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for AnalyzerPolicy {
    /// Serialize the checked canonical table without silently changing its scalar types.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let table: toml::Table = toml::from_str(&self.0).map_err(S::Error::custom)?;
        table.serialize(serializer)
    }
}

/// Only the explicitly supported analyzer owns a tool namespace in this release.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawTools {
    #[serde(rename = "bsl-analyzer")]
    pub bsl_analyzer: Option<toml::Table>,
}

/// One root-owned analyzer policy, serialized with the public tool key unchanged.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize)]
pub struct ToolSettings {
    #[serde(rename = "bsl-analyzer")]
    pub bsl_analyzer: AnalyzerPolicy,
}

impl ToolSettings {
    /// Validate raw policy once during strict manifest loading.
    pub(super) fn parse(raw: Option<RawTools>) -> Result<Self, SettingsError> {
        Ok(Self {
            bsl_analyzer: raw
                .and_then(|raw| raw.bsl_analyzer)
                .map(|table| AnalyzerPolicy::parse(&table))
                .transpose()?
                .unwrap_or_default(),
        })
    }

    /// Root ownership checks include policy-only member declarations.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bsl_analyzer.is_empty()
    }
}
