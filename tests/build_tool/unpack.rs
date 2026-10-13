//! Independent unpacking preserves native input and never performs project replacement.

use super::{command, ibcmd_fixture, report};
use crate::support::TestDir;
use std::{fs, path::Path, process::Output};

const UUID: &str = "12345678-1234-1234-1234-123456789012";

/// Carry a complete descriptor through the portable platform process in a native-looking file.
fn input(root: &Path, suffix: &str, tag: &str, extension: bool) -> Vec<u8> {
    let properties = if extension {
        "<ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>"
    } else {
        ""
    };
    let bytes = format!(
        "\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><{tag} uuid='{UUID}'><Properties><Name>Проверка</Name>{properties}</Properties></{tag}></MetaDataObject>\r\n"
    ).into_bytes();
    fs::write(root.join(format!("incoming.{suffix}")), &bytes).unwrap();
    bytes
}

/// Explicit options keep machine configuration and installed 1C out of synthetic acceptance.
fn unpack(
    root: &Path,
    locale: &str,
    suffix: &str,
    destination: &str,
    flag: Option<&str>,
) -> Output {
    let mut cmd = command(
        root,
        locale,
        &[
            "unpack",
            &format!("incoming.{suffix}"),
            destination,
            "--platform-version",
            "8.3.27.2325",
            "--format",
            "json",
            "--ibcmd",
        ],
    );
    cmd.arg(ibcmd_fixture::executable());
    if let Some(flag) = flag {
        cmd.env(flag, "1");
    }
    cmd.output().unwrap()
}

/// All four profiles and misleading suffixes use native identity rather than project discovery.
#[test]
fn exports_all_profiles_and_preserves_input_in_both_locales() {
    for locale in ["ru", "en"] {
        for (suffix, tag, extension, kind) in [
            ("cf", "Configuration", false, "configuration"),
            ("cfe", "Configuration", true, "extension"),
            ("epf", "ExternalDataProcessor", false, "processing"),
            ("erf", "ExternalReport", false, "report"),
            ("cf", "Configuration", true, "extension"),
            ("cfe", "Configuration", false, "configuration"),
        ] {
            let test = TestDir::new();
            let bytes = input(&test.0, suffix, tag, extension);
            let output = unpack(&test.0, locale, suffix, "unpacked", None);
            let value = report(&output, "success");
            assert!(output.stderr.is_empty(), "{output:?}");
            assert_eq!(value["command"], "unpack");
            assert_eq!(value["data"]["uuid"], UUID);
            assert_eq!(value["data"]["project_type"], kind);
            assert_eq!(value["data"]["project_created"], false);
            assert_eq!(value["data"]["source_format"], "designer-xml");
            let descriptor = if matches!(kind, "configuration" | "extension") {
                "Configuration.xml"
            } else {
                "incoming.xml"
            };
            assert_eq!(
                fs::read(test.0.join("unpacked").join(descriptor)).unwrap(),
                bytes
            );
            assert_eq!(
                fs::read(test.0.join(format!("incoming.{suffix}"))).unwrap(),
                bytes
            );
            assert!(!test.0.join("unpacked/probe-only.txt").exists());
            assert!(!test.0.join("unpacked/eska.toml").exists());
            assert_eq!(
                fs::read_dir(test.0.join(".eska/import")).unwrap().count(),
                1
            );
        }
    }
}

/// Existing destinations are rejected before discovery, including files, directories and links.
#[test]
fn refuses_existing_output_before_platform_and_keeps_previous_bytes() {
    for locale in ["ru", "en"] {
        let test = TestDir::new();
        fs::create_dir(test.0.join("unpacked")).unwrap();
        fs::write(test.0.join("unpacked/keep.bsl"), b"\xef\xbb\xbfkeep\r\n").unwrap();
        let first = report(&unpack(&test.0, locale, "cf", "unpacked", None), "error");
        fs::write(test.0.join("existing"), b"file").unwrap();
        let second = report(&unpack(&test.0, locale, "cf", "existing", None), "error");
        for value in [first, second] {
            assert_eq!(value["error"]["code"], "unpack_destination_exists");
        }
        assert!(!test.0.join("platform.log").exists());
        assert_eq!(
            fs::read(test.0.join("unpacked/keep.bsl")).unwrap(),
            b"\xef\xbb\xbfkeep\r\n"
        );
        assert_eq!(fs::read(test.0.join("existing")).unwrap(), b"file");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(test.0.join("absent"), test.0.join("linked")).unwrap();
            let value = report(&unpack(&test.0, locale, "cf", "linked", None), "error");
            assert_eq!(value["error"]["code"], "unpack_destination_exists");
        }
    }
}

/// A truncated probe and failed native export keep exact diagnostics and publish no partial tree.
#[test]
fn failed_exports_preserve_input_and_cleanup_staging() {
    for locale in ["ru", "en"] {
        for (flag, code) in [
            ("FAKE_IBCMD_FAIL_LOAD", "platform_command_failed"),
            ("FAKE_IBCMD_MISSING_DUMP_INFO", "unpack_descriptor_missing"),
            (
                "FAKE_IBCMD_TRUNCATED_PROBE_DESCRIPTOR",
                "unpack_descriptor_invalid",
            ),
        ] {
            let test = TestDir::new();
            let bytes = input(&test.0, "cfe", "Configuration", true);
            let output = unpack(&test.0, locale, "cfe", "unpacked", Some(flag));
            let value = report(&output, "error");
            assert_eq!(value["error"]["code"], code);
            assert!(!output.stderr.is_empty());
            assert!(!test.0.join("unpacked").exists());
            assert_eq!(fs::read(test.0.join("incoming.cfe")).unwrap(), bytes);
            assert_eq!(
                fs::read_dir(test.0.join(".eska/import")).unwrap().count(),
                1
            );
            if flag == "FAKE_IBCMD_TRUNCATED_PROBE_DESCRIPTOR" {
                let events = value["error"]["diagnostics"]["platform"]
                    .as_array()
                    .unwrap();
                assert!(events.iter().any(|event| {
                    let bytes: Vec<u8> = serde_json::from_value(event["bytes"].clone()).unwrap();
                    String::from_utf8_lossy(&bytes).contains("incomplete fixture file export")
                }));
            }
        }
    }
}

/// Human output is localized while the JSON envelope for identical failures is language-neutral.
#[test]
fn human_result_is_localized_and_failure_json_is_stable() {
    let test = TestDir::new();
    input(&test.0, "epf", "ExternalDataProcessor", false);
    for (locale, destination, text) in [
        ("ru", "ru-output", "Designer XML распакован:"),
        ("en", "en-output", "Designer XML unpacked:"),
    ] {
        let output = command(
            &test.0,
            locale,
            &[
                "unpack",
                "incoming.epf",
                destination,
                "--platform-version",
                "8.3.27.2325",
                "--ibcmd",
            ],
        )
        .arg(ibcmd_fixture::executable())
        .output()
        .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&output.stdout).contains(text));
    }
    let ru = report(&unpack(&test.0, "ru", "epf", "ru-output", None), "error");
    let en = report(&unpack(&test.0, "en", "epf", "ru-output", None), "error");
    assert_eq!(ru, en);
}

/// Wait for the exported process boundary with a deadline, cleaning up a hung test process.
fn wait_ready(child: &mut std::process::Child, ready: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !ready.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "unpack exited before readiness"
        );
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("unpack fixture readiness timeout");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// A directory created while the platform runs wins; publication preserves its complete contents.
#[test]
fn concurrent_destination_is_preserved_after_platform_completion() {
    for locale in ["ru", "en"] {
        let test = TestDir::new();
        input(&test.0, "epf", "ExternalDataProcessor", false);
        let ready = test.0.join("ready");
        let proceed = test.0.join("continue");
        let mut child = command(
            &test.0,
            locale,
            &[
                "unpack",
                "incoming.epf",
                "unpacked",
                "--platform-version",
                "8.3.27.2325",
                "--format",
                "json",
                "--ibcmd",
            ],
        )
        .arg(ibcmd_fixture::executable())
        .env("FAKE_IBCMD_IMPORT_READY", &ready)
        .env("FAKE_IBCMD_IMPORT_CONTINUE", &proceed)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
        wait_ready(&mut child, &ready);
        fs::create_dir(test.0.join("unpacked")).unwrap();
        fs::write(test.0.join("unpacked/keep"), "concurrent").unwrap();
        fs::write(proceed, "").unwrap();
        let value = report(&child.wait_with_output().unwrap(), "error");
        assert_eq!(value["error"]["code"], "unpack_destination_exists");
        assert_eq!(
            fs::read_to_string(test.0.join("unpacked/keep")).unwrap(),
            "concurrent"
        );
        assert_eq!(fs::read_dir(test.0.join("unpacked")).unwrap().count(), 1);
        assert_eq!(
            fs::read_dir(test.0.join(".eska/import")).unwrap().count(),
            1
        );
    }
}

/// Termination stops the native operation and removes its staging without publishing project files.
#[cfg(unix)]
#[test]
fn interrupted_unpack_keeps_native_input_and_creates_no_output() {
    for locale in ["ru", "en"] {
        let test = TestDir::new();
        let bytes = input(&test.0, "erf", "ExternalReport", false);
        let ready = test.0.join("ready");
        let mut child = command(
            &test.0,
            locale,
            &[
                "unpack",
                "incoming.erf",
                "unpacked",
                "--platform-version",
                "8.3.27.2325",
                "--format",
                "json",
                "--ibcmd",
            ],
        )
        .arg(ibcmd_fixture::executable())
        .env("FAKE_IBCMD_IMPORT_READY", &ready)
        .env("FAKE_IBCMD_IMPORT_CONTINUE", test.0.join("continue"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
        wait_ready(&mut child, &ready);
        assert!(
            std::process::Command::new("kill")
                .args(["-TERM", &child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let value = report(&child.wait_with_output().unwrap(), "error");
        assert_eq!(value["error"]["code"], "platform_interrupted");
        assert_eq!(fs::read(test.0.join("incoming.erf")).unwrap(), bytes);
        assert!(!test.0.join("unpacked").exists());
        assert_eq!(
            fs::read_dir(test.0.join(".eska/import")).unwrap().count(),
            1
        );
    }
}
