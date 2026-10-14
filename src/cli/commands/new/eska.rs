//! Explicit ESKA creation delegates source ownership and retains normal shell onboarding policy.

mod authority;
mod children;
mod policy;
pub(in crate::cli) mod process;
pub(in crate::cli) mod protocol;
mod publication;
mod stage;

use super::{
    NewArgs,
    artifact::{self, Failure},
};
use crate::{
    cli::{
        localization::{LocalizationValue, Localizer},
        path_output,
    },
    project::create,
    project_core::native_version,
    tool_cli::OutputFormat,
};
use serde_json::{Value, json};
use std::{
    io::{self, IsTerminal},
    path::Path,
    process::ExitCode,
};

/// Compound creation returns only after children finish; the parent retains staging/enrollment Drop.
pub(super) fn run(args: &NewArgs, base: &Path, localizer: &Localizer) -> ExitCode {
    match execute(args, base, localizer) {
        Ok(data) => {
            if matches!(args.format, OutputFormat::Json) {
                println!(
                    "{}",
                    json!({"schema_version":1,"tool":"eska","command":"new","status":"success","data":data})
                );
            } else {
                let root = Path::new(data["root"].as_str().unwrap_or_default());
                artifact::success(&localizer.format(
                    "new-eska-created",
                    &[(
                        "path",
                        LocalizationValue::Text(&path_output::render(
                            root,
                            io::stdout().is_terminal(),
                        )),
                    )],
                ));
                pins(&data, localizer);
            }
            ExitCode::SUCCESS
        }
        Err(error) => report_failure(args, &error, localizer),
    }
}

/// Failure output reports retained paths/candidate pins without implying destination ownership.
pub(super) fn report_failure(args: &NewArgs, error: &Failure, localizer: &Localizer) -> ExitCode {
    if matches!(args.format, OutputFormat::Json) {
        println!(
            "{}",
            json!({"schema_version":1,"tool":"eska","command":"new","status":"error","error":{"code":error.code},"diagnostics":error.details})
        );
    } else {
        error.report();
        if let Some(details) = &error.details {
            if let Some(root) = details["root"].as_str() {
                artifact::report(
                    "!",
                    &localizer.format(
                        "new-eska-unfinished",
                        &[(
                            "path",
                            LocalizationValue::Text(&path_output::render(
                                Path::new(root),
                                io::stderr().is_terminal(),
                            )),
                        )],
                    ),
                    "33",
                );
            }
            pins(details, localizer);
            if let Some(stage) = details["native_staging"].as_str() {
                artifact::report(
                    "!",
                    &localizer.format(
                        "new-native-staging-retained",
                        &[("path", LocalizationValue::Text(stage))],
                    ),
                    "33",
                );
            }
        }
    }
    if matches!(
        error.code,
        "options-required"
            | "project-type-invalid"
            | "workflow-invalid"
            | "workspace-workflow"
            | "platform-required"
            | "platform-version-invalid"
            | "new-source-options-incompatible"
            | "new-from-source-options-incompatible"
            | "new-empty-runner-options-incompatible"
            | "new-eska-name-invalid"
            | "new-metadata-name-invalid"
            | "new-initial-version-invalid"
            | "new-version-scheme-mismatch"
    ) {
        ExitCode::from(2)
    } else {
        ExitCode::FAILURE
    }
}

/// Capability checks and read-only policy preflight precede every parent or child mutation.
fn execute(args: &NewArgs, base: &Path, localizer: &Localizer) -> Result<Value, Failure> {
    let mut plan = policy::prepare(args, base, localizer)?;
    let (project_tool, build_tool) = children::tools(&plan, args, localizer)?;
    children::platform(&mut plan, args, localizer)?;
    plan.verify_workspace(localizer)?;
    let owned_parent = plan
        .member
        .as_ref()
        .map(|member| {
            create::prepare_member_parent(member)
                .map_err(|error| Failure::new("creation", super::present(&error, localizer)))
        })
        .transpose()?
        .unwrap_or(false);
    let mut stage = None;
    let result = (|| {
        let expected = if let Some(build_tool) = &build_tool {
            stage = Some(
                stage::NativeStage::claim(&plan.destination)
                    .map_err(|_| failure("new-native-staging-failed", localizer))?,
            );
            let staging = stage
                .as_mut()
                .ok_or_else(|| failure("new-native-staging-failed", localizer))?;
            children::unpack(build_tool, args, base, &plan, staging, localizer)?
        } else {
            children::Expected {
                kind: plan
                    .project_type
                    .ok_or_else(|| failure("new-child-response-invalid", localizer))?,
                metadata_name: plan
                    .metadata_name
                    .clone()
                    .ok_or_else(|| failure("new-child-response-invalid", localizer))?,
                uuid: None,
                version: plan
                    .version
                    .clone()
                    .ok_or_else(|| failure("new-child-response-invalid", localizer))?,
            }
        };
        plan.verify_workspace(localizer)?;
        let mut created = children::create(&project_tool, args, &plan, stage.as_ref(), localizer)
            .map_err(|error| unacknowledged(error, &plan.destination))?;
        finalize_candidate(args, &plan, &mut created, &expected, localizer)
    })();
    if let Some(member) = &plan.member {
        create::cleanup_member_parent(member, owned_parent);
    }
    match (result, stage) {
        (Ok(mut data), Some(stage)) => {
            let path = stage.root().to_owned();
            if stage.cleanup().is_err() {
                data["native_staging"] = json!(path);
                data["stage"] = json!("native-staging-cleanup");
                data["destination_removed"] = json!(false);
                return Err(failure("new-native-staging-changed", localizer).with_details(data));
            }
            Ok(data)
        }
        (Err(mut error), Some(stage)) => {
            let details = error.details.get_or_insert_with(|| json!({}));
            details["native_staging"] = json!(stage.root());
            Err(error)
        }
        (result, None) => result,
    }
}

/// Verify actual source authorities before completing shell-owned workflow, Git or enrollment.
fn finalize_candidate(
    args: &NewArgs,
    plan: &policy::Plan,
    created: &mut protocol::Created,
    expected: &children::Expected,
    localizer: &Localizer,
) -> Result<Value, Failure> {
    let result = (|| {
        let verified = authority::verify(
            created,
            expected.kind,
            &expected.metadata_name,
            expected.uuid.as_deref(),
        )
        .map_err(|error| {
            failure(
                if error == protocol::Error::ManifestChanged {
                    "new-manifest-changed"
                } else {
                    "new-source-changed"
                },
                localizer,
            )
        })?;
        let version = native_version::project_source_version(&verified.project)
            .map_err(|_| failure("new-source-changed", localizer))?;
        if version != expected.version {
            return Err(failure("new-source-changed", localizer));
        }
        if let Some(scheme) = verified.project.configuration().version_scheme() {
            version
                .validate_scheme(scheme)
                .map_err(|_| failure("new-source-changed", localizer))?;
        }
        publication::finish(plan, args, created, &verified, localizer)?;
        let mut data = json!({"root":created.root,"name":created.name.as_str(),"source":created.root.join(&created.source_relative),
            "source_format":"eska","source_layout":"queries","accepted_base":".eska/accepted",
            "accepted_base_sha256":created.accepted.sha256,"ancestor_sha256":created.ancestor.sha256()});
        data["type"] = json!(expected.kind.as_str());
        data["metadata_name"] = json!(expected.metadata_name);
        data["version"] = json!(version.to_string());
        data["version_scheme"] = json!(version.scheme().as_str());
        data["manifest_sha256"] = json!(created.manifest_sha256);
        data["project_created"] = json!(true);
        data["onboarding_complete"] = json!(true);
        data["source_verified"] = json!(true);
        data["support_approved"] = json!(false);
        data["workspace_member"] = json!(plan.member.is_some());
        Ok(data)
    })();
    result.map_err(|error: Failure| {
        let source_verified = authority::verify(
            created,
            expected.kind,
            &expected.metadata_name,
            expected.uuid.as_deref(),
        )
        .is_ok();
        let mut details = protocol::unfinished(created, error.code, source_verified);
        details["recovery"] = json!("inspect_retained_project_and_complete_onboarding");
        if let Some(detail) = error.details {
            details["detail"] = detail;
        }
        Failure {
            details: Some(details),
            ..error
        }
    })
}

/// The child may fail after publishing, so an invalid receipt never permits root cleanup.
fn unacknowledged(mut error: Failure, destination: &Path) -> Failure {
    let details = error.details.get_or_insert_with(|| json!({}));
    details["root"] = json!(destination);
    details["destination_present"] = json!(std::fs::symlink_metadata(destination).is_ok());
    details["project_created"] = Value::Null;
    details["onboarding_complete"] = json!(false);
    details["source_verified"] = json!(false);
    details["support_approved"] = json!(false);
    details["destination_removed"] = json!(false);
    details["recovery"] = json!("inspect_retained_destination_and_child_error");
    error
}

/// Shared shell errors use the same stable code as their localized key.
fn failure(code: &'static str, localizer: &Localizer) -> Failure {
    Failure::new(code, localizer.text(code))
}

/// Candidates are returned before any future edits; their presence grants no approval.
fn pins(data: &Value, localizer: &Localizer) {
    if let (Some(accepted), Some(ancestor)) = (
        data["accepted_base_sha256"].as_str(),
        data["ancestor_sha256"].as_str(),
    ) {
        eprintln!(
            "{}",
            localizer.format(
                "new-eska-candidates",
                &[
                    ("accepted", LocalizationValue::Text(accepted)),
                    ("ancestor", LocalizationValue::Text(ancestor))
                ]
            )
        );
    }
}
