//! Isolated patch generation; only a checked candidate can become a final artifact.

use super::{MD, PatchError, PatchPlan, error};
use crate::{project::build::Ibcmd, vcs::repository::Repository};
use std::{
    ffi::OsString,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct Workspace(PathBuf);

impl Drop for Workspace {
    /// Delete only the uniquely claimed workspace, including incomplete platform databases.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Build and validate a patch against its committed baseline without touching a user's infobase.
///
/// # Errors
/// Returns errors before publication for platform, source, validation or output failures.
pub fn execute(plan: &PatchPlan, tool: &Ibcmd, output: &Path) -> Result<(), PatchError> {
    validate_output(plan, tool, output)?;
    let parent = output
        .parent()
        .ok_or_else(|| error("output", output.display().to_string()))?;
    fs::create_dir_all(parent).map_err(|e| error("io", e.to_string()))?;
    let workspace = workspace(parent)?;
    tool.begin_interruptible_operation()
        .map_err(|e| error("tool", format!("{e:?}")))?;
    let base = stage_base(plan, &workspace)?;
    let candidate = build_candidate(plan, tool, &workspace, &base)?;
    if tool.was_interrupted() {
        return Err(error("tool", "interrupted"));
    }
    fs::hard_link(candidate, output).map_err(|e| error("output", e.to_string()))
}

/// Reject unsupported plans and unsafe publication paths before any write.
fn validate_output(plan: &PatchPlan, tool: &Ibcmd, output: &Path) -> Result<(), PatchError> {
    if plan.modules.is_empty() {
        return Err(error("empty", ""));
    }
    if tool.version().as_str() != "8.3.27.2325" {
        return Err(error("platform", tool.version().as_str()));
    }
    if output.extension() != Some(std::ffi::OsStr::new("cfe")) || output.exists() {
        return Err(error("output", output.display().to_string()));
    }
    for ancestor in output.ancestors() {
        if let Ok(metadata) = fs::symlink_metadata(ancestor)
            && metadata.file_type().is_symlink()
        {
            return Err(error("output", ancestor.display().to_string()));
        }
    }
    Ok(())
}

/// Materialize the exact merge-base source tree in the owned workspace.
fn stage_base(plan: &PatchPlan, workspace: &Workspace) -> Result<PathBuf, PatchError> {
    let repository = Repository::discover(&plan.repository_root)
        .map_err(|e| error("repository", format!("{e:?}")))?;
    let base = workspace.0.join("base");
    for (path, id) in &plan.source_files {
        let contents = repository
            .blob(*id)
            .map_err(|e| error("repository", format!("{e:?}")))?;
        if contents.starts_with(b"version https://git-lfs.github.com/spec/v1") {
            return Err(error("unsupported", path));
        }
        let target = base.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| error("io", e.to_string()))?;
        }
        fs::write(target, contents).map_err(|e| error("io", e.to_string()))?;
    }
    Ok(base)
}

/// Produce and validate an unpublished candidate in the temporary database.
fn build_candidate(
    plan: &PatchPlan,
    tool: &Ibcmd,
    workspace: &Workspace,
    base: &Path,
) -> Result<PathBuf, PatchError> {
    let data = workspace.0.join("data");
    let extension = workspace.0.join("extension");
    let candidate = workspace.0.join("candidate.cfe");
    let data_arg = option("--data", &data);
    let extension_arg = OsString::from(format!("--extension={}", plan.name));
    prepare_infobase(tool, workspace, base, &data_arg)?;
    create_extension(tool, workspace, plan, &data_arg, &extension_arg, &extension)?;
    validate_extension(
        tool,
        workspace,
        &data,
        &plan.name,
        &data_arg,
        &extension_arg,
    )?;
    save_candidate(tool, workspace, &data_arg, &extension_arg, &candidate)?;
    if fs::metadata(&candidate)
        .map_err(|e| error("io", e.to_string()))?
        .len()
        == 0
    {
        return Err(error("empty", ""));
    }
    Ok(candidate)
}

/// Create an isolated infobase from the exact committed baseline.
fn prepare_infobase(
    tool: &Ibcmd,
    workspace: &Workspace,
    base: &Path,
    data_arg: &OsString,
) -> Result<(), PatchError> {
    run(
        tool,
        workspace,
        "create",
        vec!["infobase".into(), "create".into(), data_arg.clone()],
    )?;
    run(
        tool,
        workspace,
        "import-base",
        vec![
            "config".into(),
            "import".into(),
            data_arg.clone(),
            base.as_os_str().to_owned(),
        ],
    )?;
    run(
        tool,
        workspace,
        "apply-base",
        vec!["config".into(), "apply".into(), data_arg.clone()],
    )
}

/// Create the platform skeleton and replace it with the generated extension sources.
fn create_extension(
    tool: &Ibcmd,
    workspace: &Workspace,
    plan: &PatchPlan,
    data_arg: &OsString,
    extension_arg: &OsString,
    extension: &Path,
) -> Result<(), PatchError> {
    run(
        tool,
        workspace,
        "create-extension",
        vec![
            "extension".into(),
            "create".into(),
            data_arg.clone(),
            format!("--name={}", plan.name).into(),
            format!("--name-prefix={}_", plan.name).into(),
            "--purpose=patch".into(),
        ],
    )?;
    run(
        tool,
        workspace,
        "export-extension",
        vec![
            "config".into(),
            "export".into(),
            data_arg.clone(),
            extension_arg.clone(),
            extension.as_os_str().to_owned(),
        ],
    )?;
    write_extension(plan, extension)?;
    run(
        tool,
        workspace,
        "import-extension",
        vec![
            "config".into(),
            "import".into(),
            data_arg.clone(),
            extension_arg.clone(),
            extension.as_os_str().to_owned(),
        ],
    )
}

/// Run all platform checks required before publishing the candidate.
fn validate_extension(
    tool: &Ibcmd,
    workspace: &Workspace,
    data: &Path,
    name: &str,
    data_arg: &OsString,
    extension_arg: &OsString,
) -> Result<(), PatchError> {
    run(
        tool,
        workspace,
        "check-extension",
        vec![
            "config".into(),
            "check".into(),
            data_arg.clone(),
            extension_arg.clone(),
        ],
    )?;
    run(
        tool,
        workspace,
        "apply-extension",
        vec![
            "config".into(),
            "apply".into(),
            data_arg.clone(),
            extension_arg.clone(),
        ],
    )?;
    designer(
        tool,
        workspace,
        data,
        name,
        "bsl",
        vec!["/CheckConfig", "-Server", "-ExternalConnection"],
    )?;
    designer(
        tool,
        workspace,
        data,
        name,
        "applicability",
        vec!["/CheckCanApplyConfigurationExtensions"],
    )
}

/// Serialize the validated extension to its unpublished candidate path.
fn save_candidate(
    tool: &Ibcmd,
    workspace: &Workspace,
    data_arg: &OsString,
    extension_arg: &OsString,
    candidate: &Path,
) -> Result<(), PatchError> {
    run(
        tool,
        workspace,
        "save",
        vec![
            "config".into(),
            "save".into(),
            data_arg.clone(),
            extension_arg.clone(),
            candidate.as_os_str().to_owned(),
        ],
    )
}

/// Claim a new staging directory beside the artifact without reusing another process's files.
fn workspace(parent: &Path) -> Result<Workspace, PatchError> {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| error("io", e.to_string()))?
        .as_nanos();
    for attempt in 0..32 {
        let path = parent.join(format!(
            ".eska-patch-{}-{time}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(Workspace(path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(error("io", e.to_string())),
        }
    }
    Err(error("io", parent.display().to_string()))
}

/// Run one platform stage and retain diagnostic output on failure.
fn run(
    tool: &Ibcmd,
    workspace: &Workspace,
    stage: &str,
    args: Vec<OsString>,
) -> Result<(), PatchError> {
    let result = tool
        .run_interruptible(args, &workspace.0.join("process.pid"), &mut |_, _| {})
        .map_err(|e| error("tool", format!("{stage}: {e:?}")))?;
    if !result.status.success() {
        return Err(error(
            "tool",
            format!(
                "{stage}: {}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            ),
        ));
    }
    Ok(())
}

/// Check both process status and Designer's independent result file.
fn designer(
    tool: &Ibcmd,
    workspace: &Workspace,
    data: &Path,
    name: &str,
    stage: &str,
    checks: Vec<&str>,
) -> Result<(), PatchError> {
    let log = workspace.0.join(format!("{stage}.log"));
    let result_file = workspace.0.join(format!("{stage}.result"));
    let mut args: Vec<OsString> = vec![
        "DESIGNER".into(),
        "/F".into(),
        data.join("db-data").into_os_string(),
        "/DisableStartupDialogs".into(),
        "/DisableStartupMessages".into(),
        "/Out".into(),
        log.clone().into_os_string(),
        "/DumpResult".into(),
        result_file.clone().into_os_string(),
    ];
    args.extend(checks.into_iter().map(OsString::from));
    args.extend(["-Extension".into(), name.into()]);
    let output = tool
        .run_designer(args, &workspace.0.join("process.pid"))
        .map_err(|e| error("tool", format!("{e:?}")))?;
    if !output.status.success()
        || fs::read_to_string(result_file)
            .unwrap_or_default()
            .trim_start_matches('\u{feff}')
            .trim()
            != "0"
    {
        return Err(error(
            "validation",
            fs::read_to_string(log)
                .unwrap_or_else(|_| String::from_utf8_lossy(&output.stderr).into_owned()),
        ));
    }
    Ok(())
}

/// Add generated module descriptors and bodies to a platform-created extension skeleton.
fn write_extension(plan: &PatchPlan, directory: &Path) -> Result<(), PatchError> {
    let path = directory.join("Configuration.xml");
    let mut xml = fs::read_to_string(&path).map_err(|e| error("io", e.to_string()))?;
    let document =
        roxmltree::Document::parse(&xml).map_err(|e| error("descriptor", e.to_string()))?;
    let child = document
        .descendants()
        .find(|n| n.has_tag_name((MD, "ChildObjects")))
        .ok_or_else(|| error("descriptor", "ChildObjects"))?;
    let end = child.range().end;
    let closing = xml[..end]
        .rfind("</")
        .ok_or_else(|| error("descriptor", "ChildObjects"))?;
    let mut children = String::new();
    for module in &plan.modules {
        write!(
            children,
            "<CommonModule xmlns=\"{MD}\">{}</CommonModule>",
            module.name
        )
        .expect("writing to String cannot fail");
        write_source(
            &directory.join(format!("CommonModules/{}.xml", module.name)),
            &module.descriptor,
        )?;
        write_source(
            &directory.join(format!("CommonModules/{}/Ext/Module.bsl", module.name)),
            &module.code,
        )?;
    }
    xml.insert_str(closing, &children);
    write_source(&path, &xml)
}

/// Keep generated 1C sources in their native BOM/CRLF representation.
fn write_source(path: &Path, text: &str) -> Result<(), PatchError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| error("io", e.to_string()))?;
    }
    let text = format!(
        "\u{feff}{}",
        text.trim_start_matches('\u{feff}')
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
    );
    fs::write(path, text).map_err(|e| error("io", e.to_string()))
}

/// Preserve arbitrary filesystem bytes as a single option value.
fn option(name: &str, path: &Path) -> OsString {
    let mut value = OsString::from(name);
    value.push("=");
    value.push(path);
    value
}
