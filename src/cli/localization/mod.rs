//! UI locale selection and embedded RU/EN translations.

mod platform;
mod platform_value_types;
mod platform_values;

pub use crate::tool_cli::localization::{
    Locale, LocalizationError, LocalizationValue, Localizer, resolve_locale,
    resolve_locale_from_environment,
};
