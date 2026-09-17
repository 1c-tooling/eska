//! Reversible text encodings shared by the versioned CLI JSON contracts.

use std::{ffi::OsStr, fmt::Write as _};

/// Keep valid UTF-8 Git text exact and percent-encode every byte otherwise.
pub(super) fn json_git_text(value: &[u8]) -> (String, &'static str) {
    std::str::from_utf8(value).map_or_else(
        |_| (percent_encode_bytes(value), "percent"),
        |value| (value.to_owned(), "utf-8"),
    )
}

/// Preserve a UTF-8 path directly and use a reversible platform encoding otherwise.
pub(super) fn json_path(path: &OsStr) -> (String, &'static str) {
    path.to_str()
        .map_or_else(|| encoded_path(path), |value| (value.to_owned(), "utf-8"))
}

#[cfg(unix)]
/// Preserve raw Unix path bytes using the same encoding as arbitrary Git text.
fn encoded_path(path: &OsStr) -> (String, &'static str) {
    use std::os::unix::ffi::OsStrExt;

    (percent_encode_bytes(path.as_bytes()), "percent")
}

#[cfg(windows)]
/// Preserve every UTF-16 code unit, including unpaired Windows surrogates.
fn encoded_path(path: &OsStr) -> (String, &'static str) {
    use std::os::windows::ffi::OsStrExt;

    let mut encoded = String::new();
    for unit in path.encode_wide() {
        write!(encoded, "%{unit:04X}").expect("writing to String cannot fail");
    }
    (encoded, "utf-16-percent")
}

/// Encode all bytes, including literal percent signs, so decoding is unambiguous.
fn percent_encode_bytes(value: &[u8]) -> String {
    let mut encoded = String::with_capacity(value.len() * 3);
    for byte in value {
        write!(encoded, "%{byte:02X}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{json_git_text, json_path};
    use std::ffi::OsStr;

    /// Unicode, percent signs and empty strings retain their original JSON values.
    #[test]
    fn unicode_text_and_paths_are_preserved_verbatim() {
        for value in ["", "src/Заказы 100%.bsl", "emoji-😀", "line\nnext"] {
            let expected = (value.to_owned(), "utf-8");
            assert_eq!(json_git_text(value.as_bytes()), expected);
            assert_eq!(json_path(OsStr::new(value)), expected);
        }
    }

    /// The fallback encodes every byte with uppercase hex, not URI escaping rules.
    #[test]
    fn non_utf8_git_text_has_a_reversible_encoding() {
        assert_eq!(
            json_git_text(b"raw-%\xFF"),
            ("%72%61%77%2D%25%FF".to_owned(), "percent")
        );
        let bytes: Vec<_> = (0..=u8::MAX).collect();
        let (encoded, encoding) = json_git_text(&bytes);
        assert_eq!(encoding, "percent");
        let decoded: Vec<_> = encoded
            .split('%')
            .skip(1)
            .map(|byte| u8::from_str_radix(byte, 16).expect("encoded byte"))
            .collect();
        assert_eq!(decoded, bytes);
    }

    /// Native Unix paths and Git text use identical byte encodings.
    #[cfg(unix)]
    #[test]
    fn non_utf8_unix_path_preserves_native_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let bytes = b"src/%\xFF.bsl";
        assert_eq!(json_path(OsStr::from_bytes(bytes)), json_git_text(bytes));
    }

    /// Windows fallback preserves code units that cannot be converted to UTF-8.
    #[cfg(windows)]
    #[test]
    fn windows_path_preserves_unpaired_surrogates() {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};

        let path = OsString::from_wide(&[0x0043, 0x003A, 0x005C, 0xD800, 0x0025]);
        assert_eq!(
            json_path(&path),
            ("%0043%003A%005C%D800%0025".to_owned(), "utf-16-percent")
        );
    }
}
