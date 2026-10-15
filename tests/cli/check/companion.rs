//! A standalone process makes admission, inherited streams and native argv observable.

use std::{
    env, fs,
    io::{self, Read, Write},
    path::PathBuf,
    process, thread,
    time::Duration,
};

/// Probe output and the requested operation are separate, with no project or analysis dependency.
fn main() {
    let root = PathBuf::from(env::var_os("ESKA_TEST_CHECK_ROOT").unwrap());
    let arguments: Vec<_> = env::args().skip(1).collect();
    if arguments.iter().any(|value| value == "--capabilities") {
        fs::write(
            root.join("probe-executable.txt"),
            env::current_exe().unwrap().to_str().unwrap(),
        )
        .unwrap();
        fs::write(root.join("probe.bin"), arguments.join("\0")).unwrap();
        let mode = env::var("ESKA_TEST_CHECK_MODE").unwrap_or_default();
        match mode.as_str() {
            "hang" => thread::sleep(Duration::from_secs(30)),
            "oversized" => {
                io::stdout().write_all(&vec![b' '; 65_537]).unwrap();
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_secs(30));
            }
            "failure" => process::exit(7),
            "malformed" => {
                println!("broken JSON");
                return;
            }
            _ => {}
        }
        let version = if mode == "version" {
            "incompatible".into()
        } else {
            env::var("ESKA_TEST_CHECK_VERSION").unwrap()
        };
        let profiles = if mode == "profile" {
            "\"semantic\""
        } else {
            "\"syntax\",\"semantic\""
        };
        println!(
            r#"{{"schema_version":1,"tool":"eska-check","command":"capabilities","status":"success","data":{{"version":"0.0.0","source_core_version":"{version}","protocol_version":1,"commands":["check"],"profiles":[{profiles}],"source_formats":["eska"],"report_schema_version":1}}}}"#
        );
        return;
    }
    fs::write(root.join("arguments.bin"), arguments.join("\0")).unwrap();
    let mut stdin = vec![];
    io::stdin().read_to_end(&mut stdin).unwrap();
    fs::write(root.join("stdin.bin"), stdin).unwrap();
    io::stdout().write_all(b"{\"checker\":true}\r\n").unwrap();
    io::stderr().write_all(b"checker stderr\r\n").unwrap();
    process::exit(23);
}
