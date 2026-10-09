//! Shared embedded translations and locale policy, with no core-domain localization.

mod locale;
mod localizer;

pub use locale::{Locale, resolve_locale, resolve_locale_from_environment};
pub use localizer::{LocalizationError, LocalizationValue, Localizer};
