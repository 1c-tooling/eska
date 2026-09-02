use std::env;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt;

use fluent::{FluentArgs, FluentBundle, FluentResource, FluentValue};

const EN_US_RESOURCE: &str = include_str!("../../locales/en-US/main.ftl");
const RU_RU_RESOURCE: &str = include_str!("../../locales/ru-RU/main.ftl");

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

/// A value supplied to a parameterized translation.
#[derive(Clone, Copy, Debug)]
pub enum LocalizationValue<'a> {
    Text(&'a str),
    Number(i64),
}

/// Immutable access to eska's embedded translations.
pub struct Localizer {
    locale: Locale,
    bundle: FluentBundle<FluentResource>,
}

impl Localizer {
    /// Creates a localizer from the translation resource embedded in the binary.
    ///
    /// # Errors
    ///
    /// This can fail only if an embedded resource or its canonical locale identifier
    /// is invalid. Tests validate both resources so installation needs no external files.
    pub fn try_new(locale: Locale) -> Result<Self, LocalizationError> {
        let source = match locale {
            Locale::RuRu => RU_RU_RESOURCE,
            Locale::EnUs => EN_US_RESOURCE,
        };
        let resource = FluentResource::try_new(source.to_owned()).map_err(|(_, errors)| {
            LocalizationError::new(format!(
                "invalid embedded {} resource: {errors:?}",
                locale.as_str()
            ))
        })?;
        let language_id = locale.as_str().parse().map_err(|error| {
            LocalizationError::new(format!(
                "invalid locale identifier {}: {error}",
                locale.as_str()
            ))
        })?;
        let mut bundle = FluentBundle::new(vec![language_id]);
        bundle.add_resource(resource).map_err(|errors| {
            LocalizationError::new(format!(
                "invalid embedded {} bundle: {errors:?}",
                locale.as_str()
            ))
        })?;

        Ok(Self { locale, bundle })
    }

    /// Returns the locale used by this localizer.
    #[must_use]
    pub const fn locale(&self) -> Locale {
        self.locale
    }

    /// Resolves a translation without parameters.
    #[must_use]
    pub fn text(&self, key: &str) -> String {
        self.format(key, &[])
    }

    /// Resolves a translation with named parameters.
    #[must_use]
    pub fn format(&self, key: &str, values: &[(&str, LocalizationValue<'_>)]) -> String {
        let Some(message) = self.bundle.get_message(key) else {
            debug_assert!(false, "missing localization key: {key}");
            return key.to_owned();
        };
        let Some(pattern) = message.value() else {
            debug_assert!(false, "localization key has no value: {key}");
            return key.to_owned();
        };

        let mut args = FluentArgs::new();
        for (name, value) in values {
            let value = match value {
                LocalizationValue::Text(value) => FluentValue::from(*value),
                LocalizationValue::Number(value) => FluentValue::from(*value),
            };
            args.set(*name, value);
        }

        let mut errors = Vec::new();
        let value = self
            .bundle
            .format_pattern(pattern, Some(&args), &mut errors);
        debug_assert!(
            errors.is_empty(),
            "failed to format localization key {key}: {errors:?}"
        );
        value.into_owned()
    }
}

/// An error in eska's embedded localization data.
#[derive(Debug)]
pub struct LocalizationError {
    message: String,
}

impl LocalizationError {
    const fn new(message: String) -> Self {
        Self { message }
    }
}

impl fmt::Display for LocalizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for LocalizationError {}

/// Extracts only the global `--lang` value needed before clap renders help.
#[must_use]
pub fn bootstrap_lang<T: AsRef<OsStr>>(args: &[T]) -> Option<String> {
    let mut arguments = args.iter().skip(1);
    while let Some(argument) = arguments.next() {
        let argument = argument.as_ref();
        if argument == "--" {
            break;
        }
        if argument == "--lang" {
            return arguments
                .next()
                .and_then(|value| value.as_ref().to_str())
                .map(str::to_owned);
        }
        if let Some(argument) = argument.to_str()
            && let Some(value) = argument.strip_prefix("--lang=")
        {
            return Some(value.to_owned());
        }
    }
    None
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

    #[test]
    fn extracts_both_bootstrap_argument_forms() {
        assert_eq!(
            bootstrap_lang(&["eska", "--help", "--lang", "ru"]),
            Some("ru".to_owned())
        );
        assert_eq!(
            bootstrap_lang(&["eska", "--lang=en", "--help"]),
            Some("en".to_owned())
        );
        assert_eq!(bootstrap_lang(&["eska", "--", "--lang", "ru"]), None);
    }

    #[test]
    fn embedded_resources_have_matching_keys_and_are_valid() {
        fn keys(resource: &str) -> Vec<&str> {
            let mut keys: Vec<_> = resource
                .lines()
                .filter_map(|line| line.split_once(" = ").map(|(key, _)| key))
                .collect();
            keys.sort_unstable();
            keys
        }

        assert_eq!(keys(EN_US_RESOURCE), keys(RU_RU_RESOURCE));
        assert!(Localizer::try_new(Locale::EnUs).is_ok());
        assert!(Localizer::try_new(Locale::RuRu).is_ok());
    }
}
