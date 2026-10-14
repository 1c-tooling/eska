//! Dependency-free inherited-stream fixture for one delegated native/source journal owner.

use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
    process, thread,
    time::Duration,
};

/// No background process is spawned; signal scenarios terminate only this fixture child.
fn main() {
    let root = PathBuf::from(env::var_os("IMPORT_FIXTURE_ROOT").unwrap());
    let arguments: Vec<_> = env::args().skip(1).collect();
    if arguments.iter().any(|argument| argument == "capabilities") {
        fs::write(root.join("probe.bin"), arguments.join("\0")).unwrap();
        capabilities();
        return;
    }
    fs::write(root.join("arguments.bin"), arguments.join("\0")).unwrap();
    #[cfg(unix)]
    if env::var_os("IMPORT_FIXTURE_SIGNAL").is_some() {
        process::Command::new("kill")
            .args(["-TERM", &process::id().to_string()])
            .status()
            .unwrap();
    }
    if env::var_os("IMPORT_FIXTURE_ERROR").is_some() {
        io::stdout().write_all(b"{\"schema_version\":1,\"tool\":\"eska-build\",\"command\":\"import\",\"status\":\"error\",\"error\":{\"code\":\"source_import_conflicts\",\"diagnostics\":{\"receipt\":\"recovery.json\"}}}\n").unwrap();
        io::stderr().write_all(b"raw child error\r\n").unwrap();
        process::exit(17);
    }
    io::stdout().write_all(b"{\"delegated\":true}\n").unwrap();
    io::stderr().write_all(b"native stderr\r\n").unwrap();
    process::exit(23);
}

/// Every field consumed by source import can be refused independently before actual invocation.
fn capabilities() {
    let mode = env::var("IMPORT_FIXTURE_MODE").unwrap_or_default();
    match mode.as_str() {
        "hang" => thread::sleep(Duration::from_secs(30)),
        "failure" => process::exit(7),
        "oversized" => {
            io::stdout().write_all(&vec![b' '; 65_537]).unwrap();
            io::stdout().flush().unwrap();
            thread::sleep(Duration::from_secs(30));
        }
        "malformed" => {
            println!("broken JSON");
            return;
        }
        _ => {}
    }
    let version = if mode == "version" {
        "0.0.0".to_owned()
    } else {
        env::var("IMPORT_FIXTURE_VERSION").unwrap()
    };
    let protocol = if mode == "protocol" { 2 } else { 1 };
    let commands = if mode == "command" {
        "[\"build\"]"
    } else {
        "[\"build\",\"import\",\"capabilities\"]"
    };
    let types = if mode == "type" {
        "[\"configuration\",\"extension\",\"processing\"]"
    } else {
        "[\"configuration\",\"extension\",\"processing\",\"report\"]"
    };
    let formats = if mode == "format" {
        "[\"designer-xml\"]"
    } else {
        "[\"eska\"]"
    };
    let import_mode = if mode == "mode" {
        "implicit_write"
    } else {
        "preview_then_explicit_write"
    };
    let ancestor = if mode == "ancestor" {
        "local_state"
    } else {
        "independently_confirmed_sha256"
    };
    let recovery = if mode == "recovery" {
        "none"
    } else {
        "eska-project recover-import"
    };
    println!(
        "{{\"schema_version\":1,\"tool\":\"eska-build\",\"command\":\"capabilities\",\"status\":\"success\",\"data\":{{\"version\":\"{version}\",\"protocol_version\":{protocol},\"commands\":{commands},\"eska_artifact_types\":{types},\"source_import_formats\":{formats},\"source_import_mode\":\"{import_mode}\",\"source_import_ancestor\":\"{ancestor}\",\"source_import_recovery\":\"{recovery}\"}}}}"
    );
}
