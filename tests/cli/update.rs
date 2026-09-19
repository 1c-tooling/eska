//! Self-update checks never use real installs or public release servers.

use crate::support::TestDir;
use serde_json::{Value, json};
use std::{fs, process::Command};

#[test]
fn unknown_install_and_invalid_version_are_locale_independent_without_a_project() {
    let root = TestDir::new();
    let mut outputs = Vec::new();
    for locale in ["ru-RU", "en-US"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&root.0)
            .env("AXOUPDATER_CONFIG_PATH", &root.0)
            .env_remove("AXOUPDATER_CONFIG_WORKING_DIR")
            .args(["--lang", locale, "update", "--check", "--format", "json"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let document: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(document["status"], "unsupported-installation");
        assert_eq!(document["method"], "unknown");
        assert!(output.stderr.is_empty());
        outputs.push(output.stdout);
        let invalid = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&root.0)
            .args([
                "--lang",
                locale,
                "update",
                "--target-version",
                "invalid",
                "--format",
                "json",
            ])
            .output()
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&invalid.stdout).unwrap(),
            json!({"schemaVersion":1,"status":"error","code":"invalid-version"})
        );
        let help = Command::new(env!("CARGO_BIN_EXE_eska"))
            .args(["--lang", locale, "update", "--help"])
            .output()
            .unwrap();
        assert!(help.status.success());
        assert!(String::from_utf8_lossy(&help.stdout).contains("--check"));
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    struct Server {
        url: String,
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
    }
    impl Server {
        /// Serve a local fake release and a fixture-only installer.
        fn new(version: &str, fail: bool) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let reply = json!({"tag_name":format!("v{version}"),"name":"fixture","url":url,"prerelease":false,
                "assets":[{"name":"eska-installer.sh","url":format!("{url}/installer"),"browser_download_url":format!("{url}/installer")}]}).to_string();
            let script = if fail {
                "#!/bin/sh\nexit 7\n".to_owned()
            } else {
                format!(
                    "#!/bin/sh\nset -eu\nprintf '#!/bin/sh\\necho eska {version}\\n' > \"$ESKA_INSTALL_DIR/bin/new-eska\"\nchmod +x \"$ESKA_INSTALL_DIR/bin/new-eska\"\nmv \"$ESKA_INSTALL_DIR/bin/new-eska\" \"$ESKA_INSTALL_DIR/bin/eska\"\n"
                )
            };
            let stop = Arc::new(AtomicBool::new(false));
            let cancelled = stop.clone();
            let thread = thread::spawn(move || {
                while !cancelled.load(Ordering::Relaxed) {
                    let Ok((mut stream, _)) = listener.accept() else {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut buffer = [0; 8192];
                    let Ok(count) = stream.read(&mut buffer) else {
                        continue;
                    };
                    let request = String::from_utf8_lossy(&buffer[..count]);
                    let body = if request.starts_with("GET /installer ") {
                        &script
                    } else {
                        &reply
                    };
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                }
            });
            Self {
                url,
                stop,
                thread: Some(thread),
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            self.thread.take().unwrap().join().unwrap();
        }
    }

    /// Claim a disposable install prefix and official-shaped receipt pointing only into this fixture.
    fn installation(root: &Path) {
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_eska"), root.join("bin/eska")).unwrap();
        fs::write(root.join("eska-receipt.json"),json!({"binaries":["eska"],"install_prefix":root,"modify_path":false,
            "source":{"app_name":"eska","owner":"1c-tooling","name":"eska","release_type":"github"},
            "version":env!("CARGO_PKG_VERSION"),"provider":{"source":"cargo-dist","version":"0.32.0"}}).to_string()).unwrap();
    }

    /// Route only this child process to the fake release server, never mutate the test runner's environment.
    fn run(root: &Path, server: &Server, args: &[&str]) -> std::process::Output {
        Command::new(root.join("bin/eska"))
            .current_dir(root)
            .env("AXOUPDATER_CONFIG_PATH", root)
            .env_remove("AXOUPDATER_CONFIG_WORKING_DIR")
            .env_remove("ESKA_INSTALLER_GITHUB_BASE_URL")
            .env("ESKA_INSTALLER_GHE_BASE_URL", &server.url)
            .args(["update", "--format", "json"])
            .args(args)
            .output()
            .unwrap()
    }

    #[test]
    fn installer_check_is_read_only_then_pinned_update_replaces_only_fixture() {
        let root = TestDir::new();
        installation(&root.0);
        let server = Server::new("99.0.0", false);
        let before = fs::read(root.0.join("bin/eska")).unwrap();
        let check = run(&root.0, &server, &["--check"]);
        assert!(
            check.status.success(),
            "{}",
            String::from_utf8_lossy(&check.stdout)
        );
        let document: Value = serde_json::from_slice(&check.stdout).unwrap();
        assert_eq!(document["status"], "update-available");
        assert_eq!(document["availableVersion"], "99.0.0");
        assert_eq!(fs::read(root.0.join("bin/eska")).unwrap(), before);
        assert!(!root.0.join(".eska-update.lock").exists());
        let update = run(&root.0, &server, &[]);
        assert!(
            update.status.success(),
            "{} {}",
            String::from_utf8_lossy(&update.stdout),
            String::from_utf8_lossy(&update.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&update.stdout).unwrap()["status"],
            "updated"
        );
        assert_eq!(
            Command::new(root.0.join("bin/eska"))
                .arg("--version")
                .output()
                .unwrap()
                .stdout,
            b"eska 99.0.0\n"
        );
    }

    #[test]
    fn current_release_is_read_only_and_concurrent_update_is_rejected() {
        let root = TestDir::new();
        installation(&root.0);
        let server = Server::new(env!("CARGO_PKG_VERSION"), false);
        let result = run(&root.0, &server, &[]);
        assert!(result.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&result.stdout).unwrap()["status"],
            "up-to-date"
        );
        assert!(!root.0.join(".eska-update.lock").exists());
        let newer = Server::new("99.0.0", false);
        let file = fs::File::create(root.0.join(".eska-update.lock")).unwrap();
        file.lock().unwrap();
        let result = run(&root.0, &newer, &[]);
        assert!(!result.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&result.stdout).unwrap()["code"],
            "busy"
        );
    }

    #[test]
    fn failed_cargo_query_uses_actual_root_without_installer_fallback() {
        use std::os::unix::fs::PermissionsExt;
        let root = TestDir::new();
        installation(&root.0);
        let key = format!(
            "eska {} (registry+https://github.com/rust-lang/crates.io-index)",
            env!("CARGO_PKG_VERSION")
        );
        fs::write(root.0.join(".crates2.json"), json!({"installs":{key:{"bins":["eska"],"features":[],"all_features":false,"no_default_features":false,"profile":"release","target":"x86_64-unknown-linux-gnu"}}}).to_string()).unwrap();
        let cargo = root.0.join("bin/cargo");
        fs::write(
            &cargo,
            b"#!/bin/sh\nprintf '%s\\n' \"$@\" > cargo-arguments\nexit 7\n",
        )
        .unwrap();
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
        let output = Command::new(root.0.join("bin/eska"))
            .current_dir(&root.0)
            .env("PATH", root.0.join("bin"))
            .env("AXOUPDATER_CONFIG_PATH", &root.0)
            .env_remove("AXOUPDATER_CONFIG_WORKING_DIR")
            .args(["update", "--check", "--format", "json"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["code"],
            "cargo-query"
        );
        assert_eq!(
            fs::read_to_string(root.0.join("cargo-arguments")).unwrap(),
            format!("install\n--list\n--root\n{}\n", root.0.display())
        );
        assert!(!root.0.join(".eska-update.lock").exists());
    }

    #[test]
    fn failed_install_and_wrong_receipt_leave_executable_unchanged() {
        let root = TestDir::new();
        installation(&root.0);
        let server = Server::new("99.0.0", true);
        let before = fs::read(root.0.join("bin/eska")).unwrap();
        let update = run(&root.0, &server, &[]);
        assert!(!update.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&update.stdout).unwrap()["code"],
            "installer"
        );
        assert_eq!(fs::read(root.0.join("bin/eska")).unwrap(), before);
        let path = root.0.join("eska-receipt.json");
        let mut receipt: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        receipt["source"]["owner"] = json!("another-owner");
        fs::write(&path, receipt.to_string()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&run(&root.0, &server, &[]).stdout).unwrap()["status"],
            "unsupported-installation"
        );
        fs::write(&path, b"broken receipt").unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&run(&root.0, &server, &[]).stdout).unwrap()["status"],
            "unsupported-installation"
        );
    }
}
