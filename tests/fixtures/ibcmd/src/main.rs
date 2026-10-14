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
        [group, subgroup, command, rest @ ..]
            if group == "infobase" && subgroup == "config" && command == "load" =>
        {
            load_configuration(rest)?;
        }
        [group, subgroup, command, rest @ ..]
            if group == "infobase" && subgroup == "config" && command == "export" =>
        {
            export_configuration(rest)?;
        }
        [group, subgroup, command, rest @ ..]
            if group == "infobase" && subgroup == "config" && command == "import" =>
        {
            import_configuration(rest);
        }
        _ => process::exit(9),
    }
    Ok(())
}

/// Model content-based CF/CFE loading independently from direct file export.
fn load_configuration(arguments: &[OsString]) -> Result<(), String> {
    if env_flag("FAKE_IBCMD_ARTIFACT_DIAGNOSTICS") {
        println!("[INFO] Loading fixture CF...");
    }
    if env_flag("FAKE_IBCMD_FAIL_LOAD") {
        return Err("[ERROR] fake load failure".to_owned());
    }
    let data = option_path(arguments, "--data=").ok_or("missing --data")?;
    let source = arguments.last().ok_or("missing file")?;
    let contents = fs::read_to_string(source).map_err(|error| error.to_string())?;
    let extension = arguments
        .iter()
        .any(|argument| argument.to_string_lossy().starts_with("--extension="));
    if contents.contains("ConfigurationExtensionPurpose") && !extension {
        // Real 1C can ignore a CFE loaded as CF and report a newly generated empty root.
        fs::write(
            data.join("loaded.cf"),
            empty_configuration("f0000000-0000-4000-8000-000000000002"),
        )
        .map_err(|error| error.to_string())?;
        return Ok(());
    }
    if contents.contains("ConfigurationExtensionPurpose") != extension {
        return Err("fixture root does not match the selected load profile".to_owned());
    }
    let suffix = if extension { "cfe" } else { "cf" };
    if Path::new(source)
        .extension()
        .is_none_or(|actual| actual != suffix)
    {
        return Err("fixture loader requires the detected native suffix".to_owned());
    }
    fs::copy(source, data.join("loaded.cf")).map_err(|error| error.to_string())?;
    Ok(())
}

/// Unpack fixture XML carried by a native-looking file without depending on a 1C license.
fn export_configuration(arguments: &[OsString]) -> Result<(), String> {
    if env_flag("FAKE_IBCMD_ARTIFACT_DIAGNOSTICS") {
        println!("[INFO] Exporting fixture XML...");
        eprintln!("[WARN] fixture export warning");
    }
    let direct = option_path(arguments, "--file=");
    let source = match &direct {
        Some(source) => source.clone(),
        None => option_path(arguments, "--data=")
            .ok_or("missing --data")?
            .join("loaded.cf"),
    };
    let destination = arguments
        .last()
        .map(PathBuf::from)
        .ok_or("missing destination")?;
    let mut contents = fs::read(&source).map_err(|error| error.to_string())?;
    let external = String::from_utf8_lossy(&contents).contains("<External");
    if direct.is_some() && !external {
        fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
        let expected =
            if String::from_utf8_lossy(&contents).contains("ConfigurationExtensionPurpose") {
                "cfe"
            } else {
                "cf"
            };
        if source.extension().is_some_and(|suffix| {
            matches!(suffix.to_str(), Some("cf" | "cfe")) && suffix != expected
        }) {
            contents = empty_configuration("f0000000-0000-4000-8000-000000000001").into_bytes();
        }
        if env_flag("FAKE_IBCMD_TRUNCATED_PROBE_DESCRIPTOR")
            && let Some(closing_tag) = contents.iter().rposition(|byte| *byte == b'<')
        {
            contents.truncate(closing_tag);
        }
        if !env_flag("FAKE_IBCMD_NO_PROBE_DESCRIPTOR") {
            fs::write(destination.join("Configuration.xml"), contents)
                .map_err(|error| error.to_string())?;
        }
        fs::write(destination.join("probe-only.txt"), b"partial export")
            .map_err(|error| error.to_string())?;
        return Err("[ERROR] incomplete fixture file export".to_owned());
    }
    if env_flag("FAKE_IBCMD_FAIL_EXPORT") {
        return Err("[ERROR] fake export failure".to_owned());
    }
    wait_for_test_release();
    if let Some(native_tree) = env::var_os("FAKE_IBCMD_NATIVE_TREE") {
        let target = if external {
            destination.parent().ok_or("missing external parent")?
        } else {
            destination.as_path()
        };
        copy_native_tree(Path::new(&native_tree), target).map_err(|error| error.to_string())?;
        if !external && !env_flag("FAKE_IBCMD_MISSING_DUMP_INFO") {
            fs::write(target.join("ConfigDumpInfo.xml"), b"<ConfigDumpInfo/>")
                .map_err(|error| error.to_string())?;
        }
        return Ok(());
    }
    if external {
        fs::write(destination.with_extension("xml"), contents).map_err(|error| error.to_string())
    } else {
        fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
        if env_flag("FAKE_IBCMD_WRONG_EXPORTED_IDENTITY") {
            contents = String::from_utf8_lossy(&contents)
                .replace("<Name>", "<Name>Changed")
                .into_bytes();
        }
        if !env_flag("FAKE_IBCMD_MISSING_DUMP_INFO") {
            fs::write(destination.join("ConfigDumpInfo.xml"), b"<ConfigDumpInfo/>")
                .map_err(|error| error.to_string())?;
        }
        fs::write(destination.join("Configuration.xml"), contents)
            .map_err(|error| error.to_string())
    }
}

/// A complete native profile keeps physical aliases, module/query bytes and resources together.
fn copy_native_tree(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_native_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A misleading platform probe and load generate different empty roots rather than the input.
fn empty_configuration(uuid: &str) -> String {
    format!(
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration uuid=\"{uuid}\"><Properties><Name>Configuration</Name></Properties></Configuration></MetaDataObject>"
    )
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
        .is_some_and(|needle| source_contains(&source, &needle))
    {
        eprintln!("fake selective import failure");
        process::exit(7);
    }

    wait_for_test_release();
    stream_test_diagnostic(&source);
    if env_flag("FAKE_IBCMD_FAIL_AFTER_STREAM") {
        eprintln!("fake import failure after diagnostic");
        process::exit(7);
    }
    if let Err(error) = write_artifact(&source, &output) {
        eprintln!("fake artifact write failed: {error}");
        process::exit(9);
    }
    println!("[WARN] fake build warning");
}

/// Match either the source path or an immediate descriptor name inside a source directory.
fn source_contains(source: &Path, needle: &OsStr) -> bool {
    let needle = needle.to_string_lossy();
    source.to_string_lossy().contains(needle.as_ref())
        || fs::read_dir(source).is_ok_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains(needle.as_ref())
            })
        })
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
    let relative = env::var("FAKE_IBCMD_STREAM_FILE").unwrap_or_else(|_| {
        "DataProcessors/РаботаСФайлами/Forms/ПрисоединенныйФайл/Ext/Help/ru.html".to_owned()
    });
    let level = env::var("FAKE_IBCMD_STREAM_LEVEL").unwrap_or_else(|_| "INFO".to_owned());
    println!("[{level}] File: {}/{relative}, checking", source.display());
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
