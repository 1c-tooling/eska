//! Explicit reference candidates span registered roots without weakening semantic identity.

use std::path::PathBuf;

use ide::{
    find_references_by_name, ReferenceAnchor, ReferenceArea, ReferenceKind, ReferencesOutcome,
    ReferencesRequest, ReferencesResult,
};
use ide_db::base_db::{SourceDatabase, SourceRoot, SourceRootId};
use ide_db::metadata::{
    CommonModuleEntry, MetadataListingData, RootKind, WorkspaceConfigsSnapshot,
};
use ide_db::RootDatabaseImpl;
use rustc_hash::FxHashSet;
use vfs::{FileId, FileSet, VfsPath};

const OWNER: FileId = FileId(0);
const BASE: FileId = FileId(1);
const A: FileId = FileId(2);
const B: FileId = FileId(3);
const SIBLING: FileId = FileId(4);
const MAIN: FileId = FileId(5);
const BODY: &str = "Процедура Пересчитать() Экспорт\n    Пересчитать();\nКонецПроцедуры\n";
const CALLER: &str = "\
Процедура Пересчитать() Экспорт
КонецПроцедуры
Процедура Прогон()
    БазовыйМодуль.Пересчитать();
    Пересчитать();
КонецПроцедуры
";
const LOCAL: &str =
    "Процедура Прогон()\n    Локальная = 1;\n    Сообщить(Локальная);\nКонецПроцедуры\n";

/// Real native fixture roots supply topology, while every candidate has its own registered root.
fn database() -> RootDatabaseImpl {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../bsl-metadata/fixtures/cfe_dependencies");
    let roots = ["base", "yaxunit", "tests_ext", "independent"]
        .map(|name| fixture.join(name).canonicalize().expect("existing native fixture"));
    let paths = roots
        .iter()
        .enumerate()
        .map(|(index, root)| ((index != 0).then(|| format!("extension-{index}")), root.clone()))
        .collect::<Vec<_>>();
    let mut db = RootDatabaseImpl::new();
    for (index, root) in roots.iter().enumerate() {
        let root_id = SourceRootId(index as u32);
        let caller = [BASE, A, B, SIBLING][index];
        let mut files = FileSet::default();
        // These equal source-relative paths must retain distinct FileIds and backlinks.
        files.insert(caller, VfsPath::from(root.join("CommonModules/Вызывающий/Ext/Module.bsl")));
        let mut texts = vec![(caller, CALLER)];
        let mut listing = MetadataListingData::default();
        if index == 0 {
            files.insert(
                OWNER,
                VfsPath::from(root.join("CommonModules/БазовыйМодуль/Ext/Module.bsl")),
            );
            files.insert(MAIN, VfsPath::from(root.join("CommonModules/БазовыйМодуль.xml")));
            texts.extend([
                (OWNER, BODY),
                (MAIN, include_str!("../../bsl-metadata/fixtures/cfe_dependencies/base/CommonModules/БазовыйМодуль.xml")),
            ]);
            listing.common_modules.push(CommonModuleEntry {
                name: "БазовыйМодуль".into(),
                main: MAIN,
                module_file: Some(OWNER),
                unread_module_file: None,
            });
        }
        db.set_source_root(root_id, SourceRoot::new_local(files));
        for (file, text) in texts {
            db.set_file_source_root(file, root_id);
            db.set_file_text(file, text);
        }
        db.set_metadata_listing(&root.to_string_lossy(), listing);
    }
    db.set_workspace_configs_snapshot(WorkspaceConfigsSnapshot {
        kinds: RootKind::from_labels(&paths),
        canonical_paths: roots.to_vec(),
        paths,
        closures: vec![vec![], vec![], vec![1], vec![]],
        topological_order: vec![0, 1, 2, 3],
        fingerprint: None,
        source_exclusions: Default::default(),
    });
    db
}

/// Anchor resolution remains positional and independent of the explicit reference search area.
fn request(roots: Option<Vec<SourceRootId>>) -> ReferencesRequest {
    ReferencesRequest {
        anchor: ReferenceAnchor::Position { file_id: OWNER, line: 0, column: 10 },
        anchor_files: None,
        area: ReferenceArea { candidate_roots: roots, files: None },
        kinds: None,
        include_declaration: true,
        max_files: 200,
    }
}

/// Compare complete native hits, not just counts that an unrelated symbol could satisfy.
fn hits(result: &ReferencesResult) -> Vec<(FileId, ReferenceKind)> {
    assert_eq!(result.outcome, ReferencesOutcome::Resolved);
    result.hits.iter().map(|hit| (hit.file_id, hit.kind)).collect()
}

/// The caller admits exactly base + A + B; another registered sibling stays outside the walk.
#[test]
fn explicit_roots_expand_candidates_and_preserve_same_path_identity() {
    let db = database();
    let mut selected = request(Some(vec![SourceRootId(0), SourceRootId(1), SourceRootId(2)]));
    selected.anchor = ReferenceAnchor::Position {
        file_id: B,
        line: 3,
        column: "    БазовыйМодуль.".encode_utf16().count() as u32,
    };
    let result = find_references_by_name(&db, &selected, &[]);
    let actual = hits(&result);
    assert_eq!(
        actual.len(),
        5,
        "owner declaration/self-call + three qualified callers: {actual:?}"
    );
    for caller in [BASE, A, B] {
        assert_eq!(
            actual.iter().filter(|(file, _)| *file == caller).count(),
            1,
            "same-name local declaration/call must not match the base symbol"
        );
        assert!(actual.contains(&(caller, ReferenceKind::Call)));
    }
    assert!(!actual.iter().any(|(file, _)| *file == SIBLING));
    assert_eq!(result.files_scanned, 4);
    assert!(!result.files_capped);

    let all = find_references_by_name(&db, &request(Some((0..4).map(SourceRootId).collect())), &[]);
    assert!(
        hits(&all).contains(&(SIBLING, ReferenceKind::Call)),
        "control: sibling really has a hit"
    );
}

/// An omitted explicit area preserves the original anchor-root behavior.
#[test]
fn default_area_keeps_the_anchor_root() {
    let db = database();
    let result = find_references_by_name(&db, &request(None), &[]);
    assert_eq!(hits(&result).len(), 3);
    assert!(result.hits.iter().all(|hit| [OWNER, BASE].contains(&hit.file_id)));
    assert_eq!(result.files_scanned, 2);
    assert!(!result.files_capped);
}

/// Root order and repeated roots cannot alter cap accounting or which files are scanned first.
#[test]
fn explicit_candidates_are_deterministic_and_deduplicated_before_the_limit() {
    let db = database();
    let roots = Some(vec![SourceRootId(2), SourceRootId(0), SourceRootId(1), SourceRootId(2)]);
    let full = find_references_by_name(
        &db,
        &ReferencesRequest { max_files: 4, ..request(roots.clone()) },
        &[],
    );
    let ordered = find_references_by_name(
        &db,
        &request(Some(vec![SourceRootId(0), SourceRootId(1), SourceRootId(2)])),
        &[],
    );
    assert_eq!(hits(&full), hits(&ordered));
    assert_eq!(full.files_scanned, 4);
    assert!(!full.files_capped, "duplicate roots do not consume the four-file budget");
    let capped =
        find_references_by_name(&db, &ReferencesRequest { max_files: 2, ..request(roots) }, &[]);
    assert_eq!(capped.files_scanned, 2);
    assert!(capped.files_capped);
    assert_eq!(hits(&capped).len(), 3);
    assert!(capped.hits.iter().all(|hit| [OWNER, BASE].contains(&hit.file_id)));
}

/// File filtering narrows the union before counting and never admits a root on its own.
#[test]
fn files_remain_an_intersection_before_the_limit() {
    let db = database();
    let mut selected = request(Some(vec![SourceRootId(0), SourceRootId(1), SourceRootId(2)]));
    selected.area.files = Some(FxHashSet::from_iter([B, SIBLING]));
    selected.max_files = 1;
    let result = find_references_by_name(&db, &selected, &[]);
    assert_eq!(hits(&result), [(B, ReferenceKind::Call)]);
    assert_eq!(result.files_scanned, 1);
    assert!(!result.files_capped);
}

/// An explicitly empty root set cannot silently restore the anchor root.
#[test]
fn empty_roots_admit_no_workspace_candidates() {
    let db = database();
    let result = find_references_by_name(&db, &request(Some(vec![])), &[]);
    assert!(hits(&result).is_empty());
    assert_eq!(result.files_scanned, 0);
    assert!(!result.files_capped);
}

/// Local bindings stay in their owning file even when the explicit workspace area excludes it.
#[test]
fn explicit_roots_do_not_change_file_local_bindings() {
    let mut db = database();
    db.set_file_text(B, LOCAL);
    let result = find_references_by_name(
        &db,
        &ReferencesRequest {
            anchor: ReferenceAnchor::Position { file_id: B, line: 1, column: 4 },
            ..request(Some(vec![]))
        },
        &[],
    );
    assert_eq!(hits(&result), [(B, ReferenceKind::Write), (B, ReferenceKind::Read)]);
    assert_eq!(result.files_scanned, 1);
    assert!(!result.files_capped);
}
