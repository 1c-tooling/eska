//! Native standalone stdio sessions use independent framing and no executable dependencies on PATH.

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use eska_project_core::{
    config::ProjectConfig,
    project::{ProjectType, create},
};
use serde_json::{Value, json};

#[path = "../../../tests/support/mod.rs"]
mod support;

#[path = "server/recovery.rs"]
mod recovery;
#[path = "server/source_navigation.rs"]
mod source_navigation;

struct Client {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Value>,
    serial: u32,
}

impl Client {
    /// Run only the project binary; the reader thread gives every response a finite deadline.
    fn new(root: &Path, locale: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_eska-project"))
            .args(["--lang", locale, "serve", "--stdio"])
            .current_dir(root)
            .env("PATH", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, output) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 {
                    return;
                }
                let length = header
                    .trim()
                    .strip_prefix("Content-Length: ")
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                assert!(length < 67_108_864);
                let mut separator = [0; 2];
                reader.read_exact(&mut separator).unwrap();
                assert_eq!(separator, *b"\r\n");
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                if send.send(serde_json::from_slice(&body).unwrap()).is_err() {
                    return;
                }
            }
        });
        Self {
            child,
            input,
            output,
            serial: 0,
        }
    }

    /// Write complete frames and correlate only responses, retaining no production protocol code.
    fn request(&mut self, method: &str, params: &Value) -> Value {
        let value = self.response(method, params);
        assert!(value.get("error").is_none(), "{value}");
        value["result"].clone()
    }

    /// Negative protocol scenarios inspect actual errors without weakening success assertions.
    fn response(&mut self, method: &str, params: &Value) -> Value {
        self.serial += 1;
        self.send(&json!({"jsonrpc":"2.0","id":self.serial,"method":method,"params":params}));
        loop {
            let value = self.output.recv_timeout(Duration::from_secs(5)).unwrap();
            if value.get("method").is_some() {
                continue;
            }
            assert_eq!(value["id"], self.serial);
            return value;
        }
    }

    /// Count UTF-8 body bytes independently of the server's encoder.
    fn send(&mut self, value: &Value) {
        let body = serde_json::to_vec(value).unwrap();
        write!(self.input, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        self.input.write_all(&body).unwrap();
        self.input.flush().unwrap();
    }

    /// Acknowledge shutdown and verify that the owned project process really exits.
    fn finish(&mut self) {
        self.request("shutdown", &json!({}));
        self.send(&json!({"jsonrpc":"2.0","method":"exit"}));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Client {
    /// Reap failed test sessions as well as successful orderly shutdowns.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// All four metadata schemas are available with only the standalone binary, in both locales.
#[test]
fn standalone_metadata_server_has_its_own_identity_and_keeps_sources_read_only() {
    let fixture = support::TestDir::new();
    let output = Command::new(env!("CARGO_BIN_EXE_eska-project"))
        .arg("__tool-contract")
        .current_dir(&fixture.0)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(output.status.success());
    let contract: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (index, kind) in [
        ProjectType::Configuration,
        ProjectType::Extension,
        ProjectType::Processing,
        ProjectType::Report,
    ]
    .into_iter()
    .enumerate()
    {
        let root = fixture.0.join(format!("Проект {index}"));
        let relative = Path::new("Выгрузка XML");
        create::create(
            &root,
            &ProjectConfig::new(kind)
                .with_source(relative.into())
                .unwrap(),
        )
        .unwrap();
        let (tag, extra, file) = match kind {
            ProjectType::Configuration => ("Configuration", "", "Configuration.xml"),
            ProjectType::Extension => (
                "Configuration",
                "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>",
                "Configuration.xml",
            ),
            ProjectType::Processing => ("ExternalDataProcessor", "", "Обработка.xml"),
            ProjectType::Report => ("ExternalReport", "", "Отчёт.xml"),
        };
        let descriptor = root.join(relative).join(file);
        let xml = format!(
            "\u{feff}<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><{tag} uuid=\"12345678-1234-1234-1234-123456789012\"><Properties><Name>Пример</Name>{extra}</Properties><ChildObjects/></{tag}></MetaDataObject>\r\n"
        );
        fs::write(&descriptor, &xml).unwrap();
        for locale in ["ru", "en"] {
            let mut client = Client::new(&root, locale);
            let hello=client.request("initialize",&json!({"apiVersion":{"major":1,"minor":12},"client":{"name":"standalone-test","version":"1"}}));
            assert_eq!(hello["server"]["name"], "eska-project");
            assert_eq!(hello["apiVersion"], contract["metadata_api"]);
            assert_eq!(hello["server"]["version"], contract["version"]);
            assert_eq!(hello["capabilities"]["selfUpdate"], false);
            assert_eq!(hello["capabilities"]["analyzerConfiguration"], true);
            let start = json!({"value":root,"encoding":"utf-8"});
            let opened = client.request(
                "workspace/open",
                &json!({"start":start,"selection":{"kind":"current"},"diskCache":false}),
            );
            assert_eq!(opened["projects"][0]["type"], kind.as_str());
            let project = &opened["projects"][0];
            let metadata=client.request("metadata/root",&json!({"sessionId":opened["sessionId"],"projectId":project["projectId"],"generation":project["generation"]}));
            assert!(metadata["node"]["id"].is_object());
            let context = client.request("analyzer/context", &json!({"start":start}));
            assert_eq!(
                context["context"]["limited_context"],
                kind != ProjectType::Configuration
            );
            client.finish();
            assert_eq!(fs::read(&descriptor).unwrap(), xml.as_bytes());
            assert!(!root.join(".git").exists());
            assert!(!root.join(".eska").exists());
            assert!(!root.join("bsl-analyzer.toml").exists());
        }
    }
}

/// The transport is explicit; ordinary command help remains localized and executable without a project.
#[test]
fn serve_requires_stdio_and_localizes_its_help() {
    for locale in ["ru", "en"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska-project"))
            .args(["--lang", locale, "serve"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("--stdio"));
        let output = Command::new(env!("CARGO_BIN_EXE_eska-project"))
            .args(["--lang", locale, "serve", "--help"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("eska-project serve"));
        assert!(help.contains(if locale == "ru" {
            "Использование"
        } else {
            "Usage"
        }));
    }
}
