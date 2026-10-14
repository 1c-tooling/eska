//! Dependency-free process fixture compiled only inside its caller's owned test directory.

use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
    process, thread,
    time::Duration,
};

/// Capabilities and build invocation have distinct observable outputs and failure controls.
fn main() {
    let root = PathBuf::from(env::var_os("FAKE_ESKA_BUILD_ROOT").unwrap());
    let arguments: Vec<_> = env::args().skip(1).collect();
    if arguments.iter().any(|argument| argument == "capabilities") {
        fs::write(
            root.join("probe-executable.txt"),
            env::current_exe().unwrap().to_str().unwrap(),
        )
        .unwrap();
        fs::write(root.join("probe.bin"), arguments.join("\0")).unwrap();
        capabilities();
        return;
    }
    fs::write(root.join("arguments.bin"), arguments.join("\0")).unwrap();
    #[cfg(unix)]
    if env::var_os("FAKE_ESKA_BUILD_SIGNAL").is_some() {
        let signal = env::var("FAKE_ESKA_BUILD_SIGNAL").unwrap();
        process::Command::new("kill")
            .arg(format!("-{signal}"))
            .arg(process::id().to_string())
            .status()
            .unwrap();
    }
    io::stdout().write_all(b"{\"delegated\":true}\n").unwrap();
    io::stderr().write_all(b"native stderr\r\n").unwrap();
    process::exit(23);
}

/// Every advertised field can fail independently while the actual operation remains unstarted.
fn capabilities() {
    let mode = env::var("FAKE_ESKA_BUILD_MODE").unwrap_or_default();
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
        "0.0.0-incompatible".to_owned()
    } else {
        env::var("FAKE_ESKA_BUILD_VERSION").unwrap()
    };
    let protocol = if mode == "protocol" { 999 } else { 1 };
    let tool = if mode == "tool" {
        "another-tool"
    } else {
        "eska-build"
    };
    let types = if mode == "artifact" {
        "\"report\""
    } else {
        "\"configuration\",\"extension\",\"processing\",\"report\""
    };
    println!(
        r#"{{"schema_version":1,"tool":"{tool}","command":"capabilities","status":"success","data":{{"version":"{version}","protocol_version":{protocol},"commands":["build"],"source_formats":["eska"],"eska_artifact_types":[{types}]}}}}"#
    );
}
