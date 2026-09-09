use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process, thread,
    time::Duration,
};

const DEFAULT_VERSION: &str = "8.3.27.2325";

/// Emulate the small `ibcmd` command surface exercised by build integration tests.
fn main() {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if let Err(error) = run(&arguments) {
        eprintln!("{error}");
        process::exit(9);
    }
}

/// Dispatch one fake command without using a platform shell.
fn run(arguments: &[OsString]) -> Result<(), String> {
    log_invocation(arguments).map_err(|error| format!("log invocation: {error}"))?;
    match arguments {
        [command] if command == "--version" => print_version(),
        [group, command, rest @ ..] if group == "infobase" && command == "create" => {
            create_infobase(rest);
        }
        [group, command, rest @ ..] if group == "config" && command == "import" => {
            import_configuration(rest);
        }
        _ => process::exit(9),
    }
    Ok(())
}

/// Append arguments in the stable form asserted by the parent integration tests.
fn log_invocation(arguments: &[OsString]) -> io::Result<()> {
    let Some(path) = env::var_os("FAKE_IBCMD_LOG") else {
        return Ok(());
    };
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    for (index, argument) in arguments.iter().enumerate() {
        if index != 0 {
            file.write_all(b" ")?;
        }
        file.write_all(argument.to_string_lossy().as_bytes())?;
    }
    file.write_all(b"\n")
}

/// Print a selectable platform version using the real process stdout channel.
fn print_version() {
    let version = env::var("FAKE_IBCMD_VERSION").unwrap_or_else(|_| DEFAULT_VERSION.to_owned());
    println!("1C ibcmd version {version}");
}

/// Create the data directory passed through `--data`.
fn create_infobase(arguments: &[OsString]) {
    let Some(data) = option_path(arguments, "--data=") else {
        process::exit(9);
    };
    if let Err(error) = fs::create_dir_all(&data) {
        eprintln!("fake infobase creation failed: {error}");
        process::exit(9);
    }
}

/// Produce a deterministic artifact or an explicitly requested failure.
fn import_configuration(arguments: &[OsString]) {
    if env_flag("FAKE_IBCMD_SLOW_IMPORT") {
        thread::sleep(Duration::from_secs(30));
        return;
    }
    let Some(output) = option_path(arguments, "--out=") else {
        process::exit(9);
    };
    let Some(source) = arguments.iter().rev().find(|argument| !is_option(argument)) else {
        process::exit(9);
    };
    let source = PathBuf::from(source);

    if env_flag("FAKE_IBCMD_FAIL_IMPORT") {
        eprintln!("fake import failure");
        process::exit(7);
    }
    if env::var_os("FAKE_IBCMD_FAIL_SOURCE_CONTAINS")
        .filter(|needle| !needle.is_empty())
        .is_some_and(|needle| {
            source
                .to_string_lossy()
                .contains(needle.to_string_lossy().as_ref())
        })
    {
        eprintln!("fake selective import failure");
        process::exit(7);
    }

    wait_for_test_release();
    stream_test_diagnostic(&source);
    if let Err(error) = write_artifact(&source, &output) {
        eprintln!("fake artifact write failed: {error}");
        process::exit(9);
    }
    println!("[WARN] fake build warning");
}

/// Coordinate a source mutation with the manifested-build scenario.
fn wait_for_test_release() {
    let Some(ready) = env::var_os("FAKE_IBCMD_IMPORT_READY") else {
        return;
    };
    let Some(proceed) = env::var_os("FAKE_IBCMD_IMPORT_CONTINUE") else {
        process::exit(9);
    };
    if let Err(error) = fs::write(&ready, b"") {
        eprintln!("fake ready marker failed: {error}");
        process::exit(9);
    }
    while !Path::new(&proceed).exists() {
        thread::sleep(Duration::from_millis(50));
    }
}

/// Emit a path-bearing line early enough to verify streaming presentation.
fn stream_test_diagnostic(source: &Path) {
    if !env_flag("FAKE_IBCMD_STREAM") {
        return;
    }
    println!(
        "[INFO] File: {}/DataProcessors/РаботаСФайлами/Forms/ПрисоединенныйФайл/Ext/Help/ru.html, checking",
        source.display()
    );
    thread::sleep(Duration::from_secs(2));
}

/// Write either fixed bytes or a file copied from the effective source snapshot.
fn write_artifact(source: &Path, output: &Path) -> io::Result<()> {
    let Some(relative) = env::var_os("FAKE_IBCMD_ARTIFACT_SOURCE_FILE") else {
        return fs::write(output, b"native-artifact");
    };
    let source_root = if source.is_file() {
        source.parent().unwrap_or(source)
    } else {
        source
    };
    fs::copy(source_root.join(relative), output).map(|_| ())
}

/// Return a path option passed as one native process argument.
fn option_path(arguments: &[OsString], prefix: &str) -> Option<PathBuf> {
    arguments.iter().find_map(|argument| {
        let value = argument.to_string_lossy();
        value.strip_prefix(prefix).map(PathBuf::from)
    })
}

/// Report whether an argument belongs to the option namespace used by the fixture.
fn is_option(argument: &OsStr) -> bool {
    argument.to_string_lossy().starts_with("--")
}

/// Interpret fixture feature flags exactly as the previous shell process did.
fn env_flag(name: &str) -> bool {
    env::var_os(name).is_some_and(|value| value == "1")
}
