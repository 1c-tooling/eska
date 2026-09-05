use std::{fs, path::Path};

use eska::project::{
    diff, discovery, object_model,
    semantic::{ChangeSet, ChangeStage, ObjectPathRole, SemanticChangeAnalyzer},
};

use crate::{
    support::TestDir,
    vcs::support::{git, repository},
};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

/// Workspace analysis preserves both stages while grouping them under one object.
#[test]
fn analyzes_workspace_stages_and_unowned_project_files() {
    let fixture = semantic_repository();
    let module = fixture.0.join("src/Catalogs/Partners/Ext/ObjectModule.bsl");
    fs::write(&module, "// staged\n").expect("write staged module");
    git(
        &fixture.0,
        &["add", "src/Catalogs/Partners/Ext/ObjectModule.bsl"],
    );
    fs::write(&module, "// worktree\n").expect("write worktree module");
    fs::write(fixture.0.join("notes.txt"), "unowned\n").expect("write project file");

    let project = discovery::discover(&fixture.0).expect("discover project");
    let object_model = object_model::discover(&project).expect("discover objects");
    let diff = diff::inspect(&project).expect("inspect workspace");
    let changes = ChangeSet::from_workspace(&diff);
    let summary = SemanticChangeAnalyzer::new(&project, &object_model).analyze(&changes);

    assert_eq!(summary.files(), 2);
    assert_eq!(summary.counts().modified, 2);
    assert_eq!(summary.counts().untracked, 1);
    assert_eq!(summary.objects().len(), 1);
    assert_eq!(summary.objects()[0].id().as_str(), "catalog:Partners");
    assert_eq!(summary.objects()[0].changes().len(), 2);
    assert!(
        summary.objects()[0]
            .changes()
            .iter()
            .all(|change| change.role() == ObjectPathRole::Module)
    );
    assert_eq!(
        summary.objects()[0].changes()[0].stage(),
        ChangeStage::Index
    );
    assert_eq!(
        summary.objects()[0].changes()[1].stage(),
        ChangeStage::Worktree
    );
    assert_eq!(summary.unowned_changes().len(), 1);
    assert_eq!(summary.unowned_changes()[0].path(), b"notes.txt".as_slice());
}

/// Revision analysis uses the same pipeline and attributes form implementation paths.
#[test]
fn analyzes_revision_changes_with_the_same_object_summary() {
    let fixture = semantic_repository();
    fs::write(
        fixture
            .0
            .join("src/Catalogs/Partners/Forms/Item/Ext/Form/Module.bsl"),
        "// changed form\n",
    )
    .expect("change form module");
    fs::write(fixture.0.join("release-notes.txt"), "changed\n").expect("write unowned file");
    git(&fixture.0, &["add", "."]);
    git(&fixture.0, &["commit", "-m", "change form"]);

    let project = discovery::discover(&fixture.0).expect("discover project");
    let object_model = object_model::discover(&project).expect("discover objects");
    let diff = diff::compare(&project, "HEAD~1", "HEAD", false).expect("compare revisions");
    let changes = ChangeSet::from_revision(&diff);
    let summary = SemanticChangeAnalyzer::new(&project, &object_model).analyze(&changes);

    assert_eq!(summary.files(), 2);
    assert_eq!(summary.objects().len(), 1);
    assert_eq!(
        summary.objects()[0].id().as_str(),
        "catalog:Partners/form:Item"
    );
    assert_eq!(
        summary.objects()[0].changes()[0].role(),
        ObjectPathRole::Module
    );
    assert_eq!(
        summary.objects()[0].changes()[0].stage(),
        ChangeStage::Revision
    );
    assert_eq!(summary.unowned_changes().len(), 1);
}

/// Build a committed Designer XML project with catalog and form modules.
fn semantic_repository() -> TestDir {
    let fixture = repository();
    fs::write(
        fixture.0.join("eska.toml"),
        "[project]\ntype = 'configuration'\n",
    )
    .expect("write project config");
    write_descriptor(
        &fixture.0,
        "src/Configuration.xml",
        "Configuration",
        "Main",
        "00000000-0000-0000-0000-000000000001",
    );
    write_descriptor(
        &fixture.0,
        "src/Catalogs/Partners.xml",
        "Catalog",
        "Partners",
        "00000000-0000-0000-0000-000000000002",
    );
    write_descriptor(
        &fixture.0,
        "src/Catalogs/Partners/Forms/Item.xml",
        "Form",
        "Item",
        "00000000-0000-0000-0000-000000000003",
    );
    write_file(
        &fixture.0,
        "src/Catalogs/Partners/Ext/ObjectModule.bsl",
        "// base\n",
    );
    write_file(
        &fixture.0,
        "src/Catalogs/Partners/Forms/Item/Ext/Form/Module.bsl",
        "// base form\n",
    );
    git(&fixture.0, &["add", "."]);
    git(&fixture.0, &["commit", "-m", "base"]);
    fixture
}

/// Write one minimal Designer metadata descriptor.
fn write_descriptor(root: &Path, path: &str, tag: &str, name: &str, uuid: &str) {
    write_file(
        root,
        path,
        &format!(
            r#"<MetaDataObject xmlns="{MD}"><{tag} uuid="{uuid}"><Properties><Name>{name}</Name></Properties></{tag}></MetaDataObject>"#
        ),
    );
}

/// Write one fixture file after creating its parent directory.
fn write_file(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture parent");
    fs::write(path, contents).expect("write fixture file");
}
