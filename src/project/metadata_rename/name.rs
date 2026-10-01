//! Identifier constraints follow the audited EDT `NameValidator`, including UTF-16 limits.

use unicode_general_category::{GeneralCategory, get_general_category};

/// An identifier failure is independent of UI locale and never silently fixes user input.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NameError {
    Empty,
    TooLong,
    InvalidStart,
    InvalidCharacter,
    ReservedProperty,
}

/// Non-global common modules exclude reserved properties, with native-verified legacy exceptions.
pub fn validate_common_module_name(name: &str) -> Result<(), NameError> {
    if include_str!("reserved_common_modules.tsv")
        .lines()
        .skip(1)
        .flat_map(|line| line.split('\t'))
        .any(|reserved| super::same_name(name, reserved))
    {
        return Err(NameError::ReservedProperty);
    }
    Ok(())
}

/// Validate one metadata identifier without trimming, normalization or case conversion.
///
/// # Errors
/// Returns the violated identifier rule. Object-specific collisions are checked by the caller.
pub fn validate_name(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.encode_utf16().count() > 80 {
        return Err(NameError::TooLong);
    }
    for (index, character) in name.chars().enumerate() {
        let letter = character.len_utf16() == 1
            && matches!(
                get_general_category(character),
                GeneralCategory::UppercaseLetter
                    | GeneralCategory::LowercaseLetter
                    | GeneralCategory::TitlecaseLetter
                    | GeneralCategory::ModifierLetter
                    | GeneralCategory::OtherLetter
            );
        let digit = index > 0
            && character.len_utf16() == 1
            && get_general_category(character) == GeneralCategory::DecimalNumber;
        if character != '_' && !letter && !digit {
            return Err(if index == 0 {
                NameError::InvalidStart
            } else {
                NameError::InvalidCharacter
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifier ceiling counts UTF-16 units, independently of UTF-8 storage.
    #[test]
    fn accepts_cyrillic_and_decimal_digits_with_the_edt_length_limit() {
        for name in ["_", "Имя_1", "Écart", "A١", &"А".repeat(80)] {
            assert_eq!(validate_name(name), Ok(()));
        }
        assert_eq!(validate_name(&"А".repeat(81)), Err(NameError::TooLong));
    }

    /// Unicode derived properties are broader than the audited identifier character classes.
    #[test]
    fn rejects_alphabetic_marks_and_nondecimal_numbers_accepted_by_rust_char_predicates() {
        for name in ["A²", "AⅣ", "A\u{0345}", "A-1", " A", "A ", "𐐀Name"] {
            assert!(validate_name(name).is_err(), "{name}");
        }
        assert_eq!(validate_name(""), Err(NameError::Empty));
        assert_eq!(validate_name("1A"), Err(NameError::InvalidStart));
    }
}
