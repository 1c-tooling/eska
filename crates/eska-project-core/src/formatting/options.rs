//! The version-one style has explicit defaults; missing overrides differ from false and zero.

use super::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndentStyle {
    Tab,
    Space,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeywordCase {
    Canonical,
    Upper,
    Lower,
    Preserve,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Wrapping {
    Fit,
    OnePerLine,
    Preserve,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Comments {
    IndentOnly,
    Preserve,
}

// One declaration keeps the public option catalogue and its sparse override in sync.
macro_rules! options {
    ($($(#[$attribute:meta])* $name:ident: $kind:ty = $default:expr),+ $(,)?) => {
        /// Complete effective policy sent to the standalone formatter.
        #[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
        #[serde(deny_unknown_fields)]
        pub struct Options { $($(#[$attribute])* pub $name: $kind),+ }

        impl Default for Options {
            /// Freeze the version-one 1C style independently of analyzer or editor defaults.
            fn default() -> Self { Self { $($name: $default),+ } }
        }

        /// Sparse configuration preserves explicitly selected false and zero values.
        #[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct Patch { $(#[serde(skip_serializing_if = "Option::is_none")] pub $name: Option<$kind>),+ }

        impl Patch {
            /// Apply only present values and record the source of every effective setting.
            pub(super) fn apply(&self, target: &mut Options, origins: &mut BTreeMap<String, String>, source: &str) {
                $(if let Some(value) = self.$name { target.$name = value; origins.insert(stringify!($name).into(), source.into()); })+
            }

            /// Merge inherited tables without treating false or zero as absent.
            pub(super) const fn overlay(&mut self, other: &Self) {
                $(if other.$name.is_some() { self.$name = other.$name; })+
            }

            /// Two explicit sources for one migrated option must agree before any manifest write.
            pub(super) fn check_conflicts(&self, other: &Self) -> Result<(), Error> {
                $(if self.$name.zip(other.$name).is_some_and(|(old, new)| old != new) {
                    return Err(Error::new("legacy_conflict", stringify!($name)));
                })+
                Ok(())
            }

            /// An empty sparse policy should not materialize a new manifest section.
            #[must_use]
            pub const fn is_empty(&self) -> bool { $(self.$name.is_none())&&+ }

            /// Every key has provenance even when its value comes from the built-in profile.
            pub(super) fn default_origins() -> BTreeMap<String, String> {
                [$( (stringify!($name).into(), "profile:1c@1".into()) ),+].into()
            }
        }
    };
}

options! {
    indent_style: IndentStyle = IndentStyle::Tab,
    indent_width: u16 = 4,
    tab_width: u16 = 4,
    continuation_indent_width: u16 = 4,
    column_limit: u16 = 120,
    keyword_case: KeywordCase = KeywordCase::Canonical,
    space_after_comma: bool = true,
    space_around_operators: bool = true,
    space_inside_parentheses: bool = false,
    space_before_call_parenthesis: bool = false,
    wrap_arguments: Wrapping = Wrapping::Fit,
    wrap_binary: Wrapping = Wrapping::Fit,
    break_before_binary_operators: bool = true,
    align_assignments: bool = false,
    max_empty_lines: u16 = 1,
    blank_lines_between_methods: u16 = 1,
    trim_trailing_whitespace: bool = true,
    insert_final_newline: bool = true,
    comments: Comments = Comments::IndentOnly,
    select_items: Wrapping = Wrapping::OnePerLine,
    clause_newlines: bool = true,
    align_aliases: bool = false,
    #[serde(default = "one_per_line")]
    condition_items: Wrapping = Wrapping::OnePerLine,
    #[serde(default = "one_per_line")]
    group_items: Wrapping = Wrapping::OnePerLine,
    #[serde(default = "one_per_line")]
    order_items: Wrapping = Wrapping::OnePerLine,
    #[serde(default = "one_blank_line")]
    blank_lines_between_queries: u16 = 1,
}

/// Newly available SDBL switches retain the documented v1 request default when absent.
const fn one_per_line() -> Wrapping {
    Wrapping::OnePerLine
}

/// A v1 caller predating SDBL-specific layout still receives the pinned query profile.
const fn one_blank_line() -> u16 {
    1
}

impl Options {
    /// Reject pathological widths before allocating indentation or processing source text.
    ///
    /// # Errors
    /// Returns the exact invalid policy field without clamping its value silently.
    pub fn validate(&self) -> Result<(), Error> {
        self.validate_values()?;
        if self.indent_style == IndentStyle::Tab
            && !self.indent_width.is_multiple_of(self.tab_width)
        {
            return Err(Error::new("invalid_option", "indent_width"));
        }
        Ok(())
    }

    /// Sparse layers can set related widths separately, so only complete policies check ratios.
    pub(super) fn validate_values(&self) -> Result<(), Error> {
        for (field, value, lower, upper) in [
            ("indent_width", self.indent_width, 1, 32),
            ("tab_width", self.tab_width, 1, 32),
            (
                "continuation_indent_width",
                self.continuation_indent_width,
                0,
                64,
            ),
            ("column_limit", self.column_limit, 0, 4096),
            ("max_empty_lines", self.max_empty_lines, 0, 16),
            (
                "blank_lines_between_queries",
                self.blank_lines_between_queries,
                0,
                16,
            ),
            (
                "blank_lines_between_methods",
                self.blank_lines_between_methods,
                0,
                16,
            ),
        ] {
            if value < lower || value > upper {
                return Err(Error::new("invalid_option", field));
            }
        }
        Ok(())
    }
}
