//! Offline ancestry capture and explicit recovery reuse core storage and journal verification.

use std::path::Path;

use serde_json::{Value, json};

use crate::{
    project_cli::args::{CaptureAncestor, RecoverImport, RecoveryAction},
    project_core::source_format::{
        ProjectSourceImportRef, SourceImportRecovery, capture_import_ancestor,
        recover_project_source_import,
    },
    tool_cli::{
        Failure,
        localization::{LocalizationValue, Localizer},
        path_text,
    },
};

/// Capture explicitly supplied source bytes in a new store and return ancestry-only authority.
pub(super) fn capture(
    args: &CaptureAncestor,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    path_text(&args.input)?;
    path_text(&args.destination)?;
    let ancestor = capture_import_ancestor(&args.input, &args.destination)?;
    let path = path_text(&args.destination)?;
    let human = localizer.format(
        "tool-project-ancestor-captured",
        &[
            ("path", LocalizationValue::Text(path)),
            ("pin", LocalizationValue::Text(ancestor.sha256())),
        ],
    );
    Ok((
        json!({"destination":path,"ancestor":ancestor,"support_approved":false}),
        human,
    ))
}

/// The caller chooses an exact project root, action and retained digest before recovery can write.
pub(super) fn recover(
    root: &Path,
    args: &RecoverImport,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    path_text(root)?;
    let reference = ProjectSourceImportRef::from_sha256(&args.receipt_sha256)?;
    let root = root
        .canonicalize()
        .map_err(|_| Failure::new("source_file_missing"))?;
    let (action, name) = match args.action {
        RecoveryAction::Complete => (SourceImportRecovery::Complete, "complete"),
        RecoveryAction::RollBack => (SourceImportRecovery::RollBack, "roll-back"),
    };
    let publication = recover_project_source_import(&root, &reference, action, &|| false)?;
    let path = path_text(&root)?;
    let human = localizer.format(
        "tool-project-import-recovered",
        &[
            ("path", LocalizationValue::Text(path)),
            ("action", LocalizationValue::Text(name)),
        ],
    );
    Ok((
        json!({"project":path,"transaction":reference,"action":name,"publication":publication}),
        human,
    ))
}
