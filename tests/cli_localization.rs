use std::process::{Command, Output};

fn eska(args: &[&str], eska_lang: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eska"));
    command.args(args).env_remove("ESKA_LANG");
    if let Some(locale) = eska_lang {
        command.env("ESKA_LANG", locale);
    }
    command.output().expect("failed to run eska")
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "eska failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).expect("help output must be UTF-8")
}

#[test]
fn russian_help_is_fully_localized() {
    let help = stdout(&eska(&["--lang", "ru", "--help"], None));

    for expected in [
        "Использование",
        "Параметры",
        "Показать справку",
        "Показать версию",
    ] {
        assert!(help.contains(expected), "missing `{expected}` in:\n{help}");
    }
    for unexpected in ["Usage:", "Options:", "Print help", "Print version"] {
        assert!(
            !help.contains(unexpected),
            "found `{unexpected}` in:\n{help}"
        );
    }
}

#[test]
fn english_help_is_fully_localized() {
    let help = stdout(&eska(&["--lang", "en", "--help"], None));

    for expected in ["Usage", "Options", "Print help", "Print version"] {
        assert!(help.contains(expected), "missing `{expected}` in:\n{help}");
    }
    assert!(!help.contains("Использование"));
}

#[test]
fn environment_selects_russian() {
    let help = stdout(&eska(&["--help"], Some("ru")));
    assert!(help.contains("Использование"));
}

#[test]
fn cli_locale_has_priority_over_environment() {
    let help = stdout(&eska(&["--lang", "ru", "--help"], Some("en")));
    assert!(help.contains("Использование"));
    assert!(!help.contains("Usage:"));
}
