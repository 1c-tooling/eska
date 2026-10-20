//! UI locale selection and embedded RU/EN translations.

mod locale;
mod localizer;
mod platform;
mod platform_value_types;
mod platform_values;

pub use locale::{Locale, resolve_locale, resolve_locale_from_environment};
pub use localizer::{LocalizationError, LocalizationValue, Localizer};
