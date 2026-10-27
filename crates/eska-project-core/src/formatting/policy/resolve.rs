//! Ordered policy layers are resolved once per source, without global mutable configuration.

use super::{Config, Effective, Language, Profile, defaults, matches, relative_path};
use crate::formatting::{Error, IndentStyle, Options, Patch};
use std::{collections::BTreeMap, path::Path};

/// Manifest text remains owned by the caller so provenance never outlives its snapshot.
pub struct Layer<'a> {
    pub root: &'a Path,
    pub name: &'a str,
    pub config: &'a Config,
    pub legacy: Patch,
}

impl<'a> Layer<'a> {
    /// Import only documented legacy BSL options; ambiguous tab counts require migration input.
    ///
    /// # Errors
    /// Rejects invalid types and policies that cannot be translated without guessing.
    pub fn new(
        root: &'a Path,
        name: &'a str,
        config: &'a Config,
        manifest: &str,
    ) -> Result<Self, Error> {
        let table: toml::Table =
            toml::from_str(manifest).map_err(|_| Error::new("invalid_config", name))?;
        let legacy = legacy(&table)?;
        let mut explicit = config.common.clone();
        explicit.overlay(&config.bsl);
        legacy.check_conflicts(&explicit)?;
        Ok(Self {
            root,
            name,
            config,
            legacy,
        })
    }
}

/// Resolve profile inheritance, source-local rules and explicit operation overrides.
///
/// # Errors
/// Returns unknown/cyclic profile, incompatible legacy option or invalid effective policy errors.
pub fn resolve(
    layers: &[Layer<'_>],
    path: &Path,
    language: Language,
    operation: &Patch,
) -> Result<Effective, Error> {
    let mut profiles: BTreeMap<String, Profile> = BTreeMap::new();
    let mut selected = "1c";
    for layer in layers {
        layer.config.validate()?;
        for (name, profile) in &layer.config.profiles {
            let current = profiles.entry(name.clone()).or_default();
            if profile.based_on.is_some() {
                current.based_on.clone_from(&profile.based_on);
            }
            current.common.overlay(&profile.common);
            current.bsl.overlay(&profile.bsl);
            current.sdbl.overlay(&profile.sdbl);
        }
        if let Some(name) = &layer.config.profile {
            selected = name;
        }
    }
    let mut result = Effective {
        schema_version: 1,
        profile: selected.into(),
        profile_version: 1,
        language,
        options: defaults(language),
        origins: Patch::default_origins(),
        excluded: false,
    };
    for name in profiles.keys() {
        let mut checked = result.clone();
        apply_profile(name, &profiles, &mut Vec::new(), &mut checked)?;
        checked.options.validate()?;
    }
    apply_profile(selected, &profiles, &mut Vec::new(), &mut result)?;
    for layer in layers {
        if language == Language::Bsl {
            apply(
                &layer.legacy,
                &mut result,
                &format!("{}:legacy", layer.name),
            );
        }
        apply(&layer.config.common, &mut result, layer.name);
        apply(
            language_patch(&layer.config.bsl, &layer.config.sdbl, language),
            &mut result,
            layer.name,
        );
    }
    for layer in layers {
        if let Some(excluded) = &layer.config.exclude {
            result.excluded = relative_path(layer.root, path)
                .is_some_and(|path| excluded.iter().any(|pattern| matches(pattern, &path)));
        }
        let Some(relative) = relative_path(layer.root, path) else {
            continue;
        };
        for (index, rule) in layer.config.overrides.iter().enumerate() {
            if rule
                .include
                .iter()
                .any(|pattern| matches(pattern, &relative))
                && !rule
                    .exclude
                    .iter()
                    .any(|pattern| matches(pattern, &relative))
            {
                let origin = format!("{}:overrides[{index}]", layer.name);
                apply(&rule.common, &mut result, &origin);
                apply(
                    language_patch(&rule.bsl, &rule.sdbl, language),
                    &mut result,
                    &origin,
                );
            }
        }
    }
    super::validate_patch(operation, Some(language))?;
    apply(operation, &mut result, "operation");
    result.options.validate()?;
    Ok(result)
}

/// Cycle checking is bounded even for deliberately adversarial manifest profiles.
fn apply_profile(
    name: &str,
    profiles: &BTreeMap<String, Profile>,
    stack: &mut Vec<String>,
    result: &mut Effective,
) -> Result<(), Error> {
    if name == "1c" {
        return Ok(());
    }
    if stack.len() >= 32 || stack.iter().any(|item| item == name) {
        return Err(Error::new("profile_cycle", name));
    }
    let profile = profiles
        .get(name)
        .ok_or_else(|| Error::new("unknown_profile", name))?;
    stack.push(name.into());
    apply_profile(
        profile.based_on.as_deref().unwrap_or("1c"),
        profiles,
        stack,
        result,
    )?;
    stack.pop();
    let origin = format!("profile:{name}@1");
    apply(&profile.common, result, &origin);
    apply(
        language_patch(&profile.bsl, &profile.sdbl, result.language),
        result,
        &origin,
    );
    Ok(())
}

/// Complete provenance is updated with each sparse layer.
fn apply(patch: &Patch, result: &mut Effective, origin: &str) {
    patch.apply(&mut result.options, &mut result.origins, origin);
}

/// Language selection never consults editor indentation or an analyzer configuration.
const fn language_patch<'a>(bsl: &'a Patch, sdbl: &'a Patch, language: Language) -> &'a Patch {
    match language {
        Language::Bsl => bsl,
        Language::Sdbl => sdbl,
    }
}

/// Preserve the documented analyzer-policy meaning without mapping a tab count to visual columns.
pub fn legacy(table: &toml::Table) -> Result<Patch, Error> {
    let Some(value) = table
        .get("tools")
        .and_then(|v| v.get("bsl-analyzer"))
        .and_then(|v| v.get("formatting"))
    else {
        return Ok(Patch::default());
    };
    let legacy = value
        .as_table()
        .ok_or_else(|| Error::new("invalid_legacy", "formatting"))?;
    if let Some(key) = legacy
        .keys()
        .find(|key| !matches!(key.as_str(), "use_tabs" | "indent_size"))
    {
        return Err(Error::new("unsupported_legacy_option", key));
    }
    let tabs = legacy
        .get("use_tabs")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| Error::new("invalid_legacy", "use_tabs"))
        })
        .transpose()?
        .unwrap_or(false);
    let size = legacy
        .get("indent_size")
        .map(|value| {
            value
                .as_integer()
                .and_then(|v| u16::try_from(v).ok())
                .ok_or_else(|| Error::new("invalid_legacy", "indent_size"))
        })
        .transpose()?
        .unwrap_or(if tabs { 1 } else { 4 });
    if tabs && size != 1 {
        return Err(Error::new("ambiguous_legacy_indent", "indent_size"));
    }
    let patch = Patch {
        indent_style: Some(if tabs {
            IndentStyle::Tab
        } else {
            IndentStyle::Space
        }),
        indent_width: Some(if tabs { 4 } else { size }),
        ..Patch::default()
    };
    let mut options = Options::default();
    patch.apply(&mut options, &mut BTreeMap::new(), "legacy");
    options.validate()?;
    Ok(patch)
}
