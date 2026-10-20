//! Locale bootstrap before command-line parsing renders help or errors.

use std::ffi::OsStr;

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
