//! Embedded Fluent resources and translation formatting.

use std::{error::Error, fmt};

use fluent::{FluentArgs, FluentBundle, FluentResource, FluentValue};

use super::Locale;

const EN_US_RESOURCE: &str = include_str!("../../../locales/en-US/main.ftl");
const RU_RU_RESOURCE: &str = include_str!("../../../locales/ru-RU/main.ftl");

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

#[cfg(test)]
mod tests {
    use super::*;

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
