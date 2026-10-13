//! Unpack requires a native file and a new output directory, independent of project discovery.

use std::io::{self, Write};

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    build_cli::{
        args::{Cli, Unpack},
        errors,
    },
    build_core::{
        Ibcmd, PlatformVersion, ProcessStream,
        artifact::{PreparedArtifact, unpack_destination},
        machine,
    },
    tool_cli::{
        Failure, OutputFormat,
        localization::{LocalizationValue, Localizer},
        path_text,
    },
};

#[derive(Serialize)]
struct PlatformEvent {
    stream: &'static str,
    bytes: Vec<u8>,
}

/// Validate destination and platform options before unpacking into owned staging and publishing XML.
pub(super) fn execute(
    cli: &Cli,
    args: &Unpack,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    let destination = unpack_destination(&args.destination).map_err(errors::unpack)?;
    let path = path_text(&destination)?;
    let version = PlatformVersion::parse(&args.platform_version)
        .map_err(|_| Failure::new("platform_version_invalid").at("platform_version"))?;
    let options = machine::tool_options(
        args.ibcmd.clone(),
        args.platform_arch.clone(),
        args.distrobox.clone(),
    )
    .map_err(errors::machine)?;
    let tool = Ibcmd::discover(&version, &options).map_err(errors::tool)?;
    let mut events = Vec::new();
    let prepared = PreparedArtifact::unpack(&args.file, &destination, &tool, |stream, bytes| {
        if matches!(cli.format, OutputFormat::Human) {
            let _ = io::stderr().lock().write_all(bytes);
        }
        events.push(PlatformEvent {
            stream: match stream {
                ProcessStream::Stdout => "stdout",
                ProcessStream::Stderr => "stderr",
            },
            bytes: bytes.to_vec(),
        });
    });
    let prepared = prepared
        .and_then(|prepared| {
            prepared.publish_sources(&destination, &|| tool.was_interrupted())?;
            Ok(prepared)
        })
        .map_err(|error| {
            let failure = errors::unpack(error);
            Failure {
                diagnostics: json!({"detail":failure.diagnostics,"platform":events}),
                ..failure
            }
        })?;
    Ok((
        json!({
            "destination":path,
            "project_type":prepared.identity().project_type.as_str(),
            "uuid":prepared.identity().uuid,
            "name":prepared.identity().name,
            "files":prepared.files(),
            "platform_version":tool.version().as_str(),
            "runner":tool.runner_kind(),
            "source_format":"designer-xml",
            "project_created":false,
            "platform":events,
        }),
        localizer.format(
            "tool-build-unpacked",
            &[("path", LocalizationValue::Text(path))],
        ),
    ))
}
