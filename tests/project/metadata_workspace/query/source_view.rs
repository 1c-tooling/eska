use super::*;
use crate::backend::project::metadata_workspace::{
    MAX_SOURCE_SNAPSHOT_BYTES, NativeSourceSnapshot, QueryOwnerFormat, QueryOwnerRestrictions,
    QuerySession, SourceAccessRequest, SourceSnapshotError, SourceSnapshotOrigin,
};
use std::sync::Arc;

/// A binary-safe source view retains exact disk bytes and never becomes an instruction to modify that source.
#[test]
fn common_source_snapshot_keeps_disk_bytes_and_native_overlay_separate() {
    let (directory, mut workspace, _, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let native = "Процедура Тест()\r\n// dirty 😀\r\nКонецПроцедуры";
    let disk = project.source_snapshot(&path, None).unwrap();
    assert_eq!(disk.origin, SourceSnapshotOrigin::Disk);
    assert_eq!(disk.bytes(), INPUT.as_bytes());
    assert_eq!(disk.snapshot, snapshot(INPUT));
    assert_eq!(disk.version, None);
    assert!(disk.protection.writable());
    let overlay = project
        .source_snapshot(
            &path,
            Some(NativeSourceSnapshot {
                version: 7,
                text: Arc::from(native),
                user_read_only: true,
            }),
        )
        .unwrap();
    assert_eq!(overlay.origin, SourceSnapshotOrigin::Editor);
    assert_eq!(overlay.bytes(), native.as_bytes());
    assert_eq!(overlay.version, Some(7));
    assert!(!overlay.protection.writable());
    assert!(overlay.protection.user_read_only);
    assert_eq!(
        fs::read(directory.0.join("src").join(&path)).unwrap(),
        INPUT.as_bytes()
    );
    let binary = b"\x00\xff\xef\xbb\xbf\r\n";
    fs::write(directory.0.join("src").join(&path), binary).unwrap();
    let binary_view = project.source_snapshot(&path, None).unwrap();
    assert_eq!(binary_view.bytes(), binary);
    assert_eq!(
        disk.bytes(),
        INPUT.as_bytes(),
        "an older immutable view does not follow the disk"
    );
}

/// Temporarily disabling ordinary support enforcement cannot grant another operation the query owner's capability.
#[test]
fn common_source_protection_combines_owned_user_and_support_restrictions() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let request = SourceAccessRequest {
        path: path.clone(),
        user_read_only: false,
    };
    let document = project.query_document(&node, &path, 7, None).unwrap();
    let selected = identity(&document);
    let mut session = QuerySession::open(
        project,
        document.clone(),
        selected.clone(),
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions {
            user_read_only: false,
            owner_lock: Some(19),
        },
    )
    .unwrap();
    for enforce in [true, false] {
        let state = project
            .source_protection(std::slice::from_ref(&request), enforce)
            .unwrap()
            .remove(0);
        assert!(!state.writable());
        assert!(!state.support.read_only);
        assert_eq!(state.restrictions.len(), 1);
        assert_eq!(
            Some(state.restrictions[0].id.get()),
            session.source_restriction_id()
        );
        assert_eq!(state.restrictions[0].source, path);
    }
    session.close();
    assert!(
        project
            .source_protection(std::slice::from_ref(&request), true)
            .unwrap()[0]
            .writable()
    );
    let root = "11111111-1111-1111-1111-111111111111";
    fs::write(directory.0.join("src/Ext/ParentConfigurations.bin"),
        format!("{{6,0,1,{root},0,{root},\"1\",\"Vendor\",\"Test\",1,0,0,{root},{root},0,0,0,1,0,0,0,1,0,1,0,1,1,1,1}}" )).unwrap();
    let state = project
        .source_protection(std::slice::from_ref(&request), true)
        .unwrap()
        .remove(0);
    assert!(state.support.read_only);
    assert!(!state.writable());
    assert!(project.source_protection(&[request], false).unwrap()[0].writable());
    assert!(matches!(
        project.plan_query(
            &document,
            revision(&document),
            &selected,
            "ВЫБРАТЬ 9",
            &[],
            None
        ),
        Err(QueryDocumentError::ReadOnly)
    ));
}

/// A query's parent view is its complete captured dirty owner, even when an external change makes application stale.
#[test]
fn common_source_session_snapshot_preserves_the_retained_parent() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let input = INPUT.replace("// keep", "// unsaved parent 😀");
    let document = project
        .query_document(&node, &path, 7, Some(&input))
        .unwrap();
    let selected = identity(&document);
    let mut session = QuerySession::open(
        project,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions {
            user_read_only: false,
            owner_lock: Some(19),
        },
    )
    .unwrap();
    fs::write(
        directory.0.join("src").join(&path),
        "// externally changed\n",
    )
    .unwrap();
    let view = session.source_snapshot(project).unwrap();
    assert_eq!(view.origin, SourceSnapshotOrigin::QuerySession);
    assert_eq!(view.bytes(), input.as_bytes());
    assert_eq!(view.version, Some(7));
    assert!(!view.protection.writable());
    assert_eq!(view.protection.restrictions.len(), 1);
    session.close();
    assert!(session.source_snapshot(project).is_err());
    assert_eq!(
        view.bytes(),
        input.as_bytes(),
        "already captured read-only bytes remain inspectable after release"
    );
}

/// Unknown identities, source escapes and invalid/oversized native snapshots never become file-reader capabilities.
#[test]
fn common_source_snapshots_validate_paths_native_counters_and_limits() {
    let (directory, mut workspace, _, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    for invalid in [
        Path::new("../eska.toml"),
        Path::new(".eska/private"),
        Path::new("unknown.txt"),
        directory.0.as_path(),
    ] {
        assert!(
            project.source_snapshot(invalid, None).is_err(),
            "{invalid:?}"
        );
    }
    assert!(matches!(
        project.source_snapshot(
            &path,
            Some(NativeSourceSnapshot {
                version: 0,
                text: Arc::from(""),
                user_read_only: false
            })
        ),
        Err(SourceSnapshotError::InvalidEditorState)
    ));
    let maximum = "x".repeat(MAX_SOURCE_SNAPSHOT_BYTES);
    assert_eq!(
        project
            .source_snapshot(
                &path,
                Some(NativeSourceSnapshot {
                    version: 1,
                    text: Arc::from(maximum.as_str()),
                    user_read_only: false
                })
            )
            .unwrap()
            .bytes()
            .len(),
        MAX_SOURCE_SNAPSHOT_BYTES
    );
    let oversized = maximum + "x";
    assert!(matches!(
        project.source_snapshot(
            &path,
            Some(NativeSourceSnapshot {
                version: 1,
                text: Arc::from(oversized.as_str()),
                user_read_only: false
            })
        ),
        Err(SourceSnapshotError::TooLarge)
    ));
    fs::write(directory.0.join("src").join(&path), oversized).unwrap();
    assert!(matches!(
        project.source_snapshot(&path, None),
        Err(SourceSnapshotError::TooLarge)
    ));
}

/// Declared-looking aliases cannot turn an outside file into a protected source snapshot.
#[cfg(unix)]
#[test]
fn common_source_snapshots_reject_aliased_source_files() {
    let (directory, mut workspace, _, path) = setup();
    let target = directory.0.join("outside.txt");
    fs::write(&target, "outside private content").unwrap();
    let module = directory.0.join("src").join(&path);
    fs::remove_file(&module).unwrap();
    std::os::unix::fs::symlink(target, module).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    assert!(project.source_snapshot(&path, None).is_err());
}

/// Reading current policy before a delayed file notification must not consume that notification's invalidation.
#[test]
fn source_snapshot_read_cannot_hide_a_later_support_rules_event() {
    for initially_present in [true, false] {
        let (directory, mut workspace, _, path) = setup();
        let rules = directory.0.join("src/Ext/ParentConfigurations.bin");
        if !initially_present {
            fs::remove_file(&rules).unwrap();
        }
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        project.source_snapshot(&path, None).unwrap();
        let generation = project.generation();
        let root = "11111111-1111-1111-1111-111111111111";
        fs::write(&rules, format!("{{6,0,1,{root},0,{root},\"1\",\"Vendor\",\"Test\",1,0,0,{root},{root},0,0,0,1,0,0,0,1,0,1,0,1,1,1,1}}")).unwrap();
        assert!(
            project
                .source_snapshot(&path, None)
                .unwrap()
                .protection
                .support
                .read_only
        );
        let event = [PathBuf::from("Ext/ParentConfigurations.bin")];
        let changed = project.changed_paths(&event).unwrap();
        assert!(
            changed.generation > generation,
            "a policy read cannot acknowledge an undispatched event"
        );
        assert_eq!(
            project.changed_paths(&event).unwrap().generation,
            changed.generation,
            "identical bytes are still a duplicate after the event has been acknowledged"
        );
    }
}
