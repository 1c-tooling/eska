use std::env;

use eska::cli::Cli;
use eska::localization::{Localizer, bootstrap_lang, resolve_locale_from_environment};

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let cli_locale = bootstrap_lang(&args);
    let locale = resolve_locale_from_environment(cli_locale.as_deref());
    let localizer = Localizer::try_new(locale).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    Cli::parse_localized(args, &localizer);
}
