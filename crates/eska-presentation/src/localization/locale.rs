//! Supported locales and the CLI/environment/system selection policy.

use std::{
    env,
    ffi::{OsStr, OsString},
};

/// A locale supported by eska's human-facing interface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Locale {
    RuRu,
    #[default]
    EnUs,
}

impl Locale {
    /// Normalizes one of the explicitly supported locale spellings.
    #[must_use]
    pub fn from_locale_name(value: &str) -> Option<Self> {
        let base = value
            .split(['.', '@'])
            .next()
            .unwrap_or(value)
            .replace('_', "-")
            .to_ascii_lowercase();

        match base.as_str() {
            "ru" | "ru-ru" => Some(Self::RuRu),
            "en" | "en-us" => Some(Self::EnUs),
            _ => None,
        }
    }

    /// Returns the canonical locale identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RuRu => "ru-RU",
            Self::EnUs => "en-US",
        }
    }
}

/// Resolves a locale from already obtained values in descending priority.
///
/// Supplying values directly keeps this policy independent from the test machine.
#[must_use]
pub fn resolve_locale(
    cli_locale: Option<&str>,
    environment_locale: Option<&str>,
    system_locale: Option<&str>,
) -> Locale {
    cli_locale
        .or(environment_locale)
        .or(system_locale)
        .and_then(Locale::from_locale_name)
        .unwrap_or_default()
}

/// Resolves the locale using `ESKA_LANG`, the operating system, then `en-US`.
#[must_use]
pub fn resolve_locale_from_environment(cli_locale: Option<&str>) -> Locale {
    let environment_locale: Option<OsString> = env::var_os("ESKA_LANG");
    let system_locale = sys_locale::get_locale();
    resolve_locale(
        cli_locale,
        environment_locale.as_deref().and_then(OsStr::to_str),
        system_locale.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_supported_locale_names() {
        for value in ["ru", "ru-RU", "ru_RU", "ru_RU.UTF-8"] {
            assert_eq!(Locale::from_locale_name(value), Some(Locale::RuRu));
        }
        for value in ["en", "en-US", "en_US", "en_US.UTF-8"] {
            assert_eq!(Locale::from_locale_name(value), Some(Locale::EnUs));
        }
    }

    #[test]
    fn resolves_locale_in_priority_order() {
        assert_eq!(
            resolve_locale(Some("ru"), Some("en"), Some("en")),
            Locale::RuRu
        );
        assert_eq!(resolve_locale(None, Some("ru"), Some("en")), Locale::RuRu);
        assert_eq!(resolve_locale(None, None, Some("ru")), Locale::RuRu);
        assert_eq!(resolve_locale(None, None, Some("de-DE")), Locale::EnUs);
        assert_eq!(resolve_locale(None, None, None), Locale::EnUs);
    }
}
