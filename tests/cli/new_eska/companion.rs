//! A dependency-free wrapper exposes exact-byte intervention between child publication and parent.

use std::{env, fs, io::{self, Write}, path::PathBuf, process::{self, Command}, thread, time::Duration};

/// The wrapper only modifies paths in its caller's exclusively owned test directory.
fn main() {
    let arguments: Vec<_> = env::args().skip(1).collect();
    let root = PathBuf::from(env::var_os("NEW_FIXTURE_ROOT").unwrap());
    let mode = env::var("NEW_FIXTURE_MODE").unwrap_or_default();
    if arguments.iter().any(|argument| argument == "capabilities") {
        fs::write(root.join("probe.bin"), arguments.join("\0")).unwrap();
        match mode.as_str() {
            "probe-hang" => thread::sleep(Duration::from_secs(30)),
            "probe-oversized" => {
                io::stdout().write_all(&vec![b' '; 65_537]).unwrap();
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_secs(30));
            }
            "probe-version" => { println!("{{\"schema_version\":1,\"tool\":\"eska-project\",\"command\":\"capabilities\",\"status\":\"success\",\"data\":{{\"version\":\"0.0.0\",\"protocol_version\":1}}}}"); return; }
            _ => {}
        }
    }
    let operation = arguments.iter().position(|argument| matches!(argument.as_str(), "new" | "import"));
    let destination = operation.map(|index| PathBuf::from(&arguments[index + if arguments[index] == "import" { 2 } else { 1 }]));
    if mode == "foreign" && let Some(destination) = &destination {
        fs::create_dir(destination).unwrap();
        fs::write(destination.join("user.txt"), b"foreign publication").unwrap();
        println!("invalid receipt");
        return;
    }
    let output = Command::new(env::var_os("NEW_FIXTURE_REAL_PROJECT").unwrap()).args(&arguments).output().unwrap();
    if let Some(destination) = &destination {
        fs::write(root.join("arguments.bin"), arguments.join("\0")).unwrap();
        assert!(output.status.success(), "{output:?}");
        match mode.as_str() {
            "manifest" => append(&destination.join("eska.toml"), "\n# intervening user edit\n"),
            "source" => append(&destination.join("src/configuration.yaml"), "\n# intervening source bytes\n"),
            "accepted" => append(&destination.join(".eska/accepted/accepted.json"), "changed"),
            "state" => fs::write(destination.join(".eska/source-import/state.json"), "changed control state").unwrap(),
            "git" => {
                fs::create_dir(destination.join(".git")).unwrap();
                fs::write(destination.join(".git/foreign.txt"), "foreign Git metadata").unwrap();
            }
            "workspace" => append(&root.join("eska.toml"), "\n# intervening workspace edit\n"),
            "stage" => {
                let index = operation.unwrap();
                let input = PathBuf::from(&arguments[index + 1]);
                fs::write(input.parent().unwrap().join("intervening.txt"), "retain staging edit").unwrap();
            }
            "failed-ack" => { io::stdout().write_all(&output.stdout).unwrap(); process::exit(23); }
            _ => {}
        }
        fs::write(destination.join("user.txt"), "retain intervening file").unwrap();
    }
    io::stdout().write_all(&output.stdout).unwrap();
    io::stderr().write_all(&output.stderr).unwrap();
    process::exit(output.status.code().unwrap_or(1));
}

/// Preserve all original bytes while making the final manifest/source fence visibly stale.
fn append(path: &std::path::Path, value: &str) {
    let mut text = fs::read_to_string(path).unwrap();
    text.push_str(value);
    fs::write(path, text).unwrap();
}
