//! UI locale selection and embedded RU/EN translations.

mod locale;
mod localizer;

pub use locale::{Locale, resolve_locale, resolve_locale_from_environment};
pub use localizer::{LocalizationError, LocalizationValue, Localizer};
