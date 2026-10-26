//! Installation capabilities are independent of release versions and human help.

use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{self, Write},
    process::ExitCode,
};

pub const SCHEMA_VERSION: u64 = 2;
pub const TARGET: &str = env!("ESKA_BUILD_TARGET");
pub const CONFIG_SCHEMA: &str = "designer-xml-2";
pub const BRIDGE_SCHEMA: u64 = 1;

/// Answer only the exact machine entry, before locale, discovery, signals or native probes.
#[must_use]
pub fn respond(
    args: &[OsString],
    tool: &str,
    version: &str,
    metadata_api: Option<(u64, u64)>,
) -> Option<ExitCode> {
    if args.len() != 2 || args[1] != "__tool-contract" {
        return None;
    }
    let value = json!({
        "schema_version": SCHEMA_VERSION,
        "tool": tool,
        "target": TARGET,
        "version": version,
        "config_schema": CONFIG_SCHEMA,
        "bridge_schema": BRIDGE_SCHEMA,
        "metadata_api": metadata_api.map(|(major,minor)| json!({"major":major,"minor":minor})),
    });
    let mut output = io::stdout().lock();
    let result = serde_json::to_writer(&mut output, &value)
        .map_err(io::Error::other)
        .and_then(|()| output.write_all(b"\n"));
    Some(if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Check supported schemas rather than assuming that equal or newer release numbers interoperate.
#[must_use]
pub fn compatible(value: &Value, expected_tool: &str) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.len() == 7
        && [
            "schema_version",
            "tool",
            "target",
            "version",
            "config_schema",
            "bridge_schema",
            "metadata_api",
        ]
        .iter()
        .all(|key| object.contains_key(*key))
        && value["schema_version"] == SCHEMA_VERSION
        && value["tool"] == expected_tool
        && value["target"] == TARGET
        && value["version"].as_str().is_some_and(stable_version)
        && value["config_schema"] == CONFIG_SCHEMA
        && value["bridge_schema"] == BRIDGE_SCHEMA
        && if expected_tool == "eska-project" {
            value["metadata_api"].as_object().is_some_and(|api| {
                api.len() == 2
                    && api.get("major").and_then(Value::as_u64) == Some(1)
                    && api
                        .get("minor")
                        .and_then(Value::as_u64)
                        .is_some_and(|minor| minor >= 12)
            })
        } else {
            value["metadata_api"].is_null()
        }
}

/// Stable numeric triples exclude shell fragments, prereleases and ambiguous leading zeros.
fn stable_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && (*part == "0" || !part.starts_with('0'))
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && part.parse::<u64>().is_ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compatible minor releases remain selectable; changed schemas and partial replies do not.
    #[test]
    fn contract_checks_capabilities_and_exact_identity() {
        let value = json!({"schema_version":SCHEMA_VERSION,"tool":"eska-project","target":TARGET,"version":"0.14.3",
            "config_schema":CONFIG_SCHEMA,"bridge_schema":1,"metadata_api":{"major":1,"minor":12}});
        assert!(compatible(&value, "eska-project"));
        for (key, replacement) in [
            ("schema_version", json!(3)),
            ("schema_version", json!(1)),
            ("tool", json!("eska")),
            ("version", json!("0.14.3-dev")),
            ("config_schema", json!("designer-xml-1")),
            ("bridge_schema", json!(2)),
            ("metadata_api", json!({"major":2,"minor":12})),
            ("metadata_api", json!({"major":1,"minor":11})),
            ("metadata_api", json!({"wrong":1,"minor":12})),
        ] {
            let mut changed = value.clone();
            changed[key] = replacement;
            assert!(!compatible(&changed, "eska-project"), "{changed}");
        }
        let mut newer = value.clone();
        newer["version"] = json!("0.15.0");
        newer["metadata_api"]["minor"] = json!(13);
        assert!(compatible(&newer, "eska-project"));
        let mut partial = value;
        partial.as_object_mut().unwrap().remove("bridge_schema");
        assert!(!compatible(&partial, "eska-project"));
    }

    /// An option value equal to the machine entry must remain an ordinary project path.
    #[test]
    fn unrelated_arguments_are_not_intercepted() {
        for args in [
            vec!["eska"],
            vec!["eska", "--project-dir", "__tool-contract"],
            vec!["eska", "__tool-contract", "--extra"],
        ] {
            let args: Vec<_> = args.into_iter().map(OsString::from).collect();
            assert!(respond(&args, "eska", "0.14.3", None).is_none());
        }
    }
}
