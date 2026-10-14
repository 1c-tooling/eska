//! Real checkout proves that initial snapshots and raw history bypass text/filter/encoding conversion.

use super::*;
use crate::project_core::{
    ProjectName,
    creation::import_query_project,
    source_format::{
        ImportAncestor, ImportAncestorRef, capture_import_ancestor, prepare_project_source_import,
        project_import_ancestor,
    },
};

struct Fixture {
    repository: PathBuf,
    project: Project,
    scope: ProjectScope,
    accepted: AcceptedRef,
    initial: ImportAncestorRef,
}

/// Scoped child attributes override destructive parent rules under an actual member-relative root.
fn fixture(test: &TestDir, isolation: &Path, member: bool) -> Fixture {
    let seed = test.0.join("seed");
    git_fixture(&seed, isolation);
    let repository = test.0.join("repository");
    fs::create_dir(&repository).unwrap();
    let (root, history, scope) = if member {
        fs::create_dir(repository.join("projects")).unwrap();
        fs::write(
            repository.join("eska.toml"),
            "[workspace]\nmembers = ['projects/member [x]']\n",
        )
        .unwrap();
        (
            repository.join("projects/member [x]"),
            "projects/member [x]/.eska/source-import",
            ProjectScope::Member(ProjectName::parse("member".into()).unwrap()),
        )
    } else {
        (
            repository.join("project"),
            ".eska/source-import",
            ProjectScope::Standalone,
        )
    };
    let imported = import_query_project(
        &seed.join("native"),
        &root,
        ProjectName::parse("member".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let repository = if member { repository } else { root.clone() };
    git(&repository, isolation, &["init", "--initial-branch=main"]);
    git(
        &repository,
        isolation,
        &[
            "config",
            "filter.eska-test.clean",
            "git hash-object --stdin",
        ],
    );
    let unsafe_rule = format!(
        "* text=auto\n{} text ident filter=eska-test working-tree-encoding=UTF-16LE\n",
        quoted_pattern(&SourcePath::parse(history).unwrap())
    );
    fs::write(repository.join(ATTRIBUTES), unsafe_rule).unwrap();
    if member {
        fs::write(root.join(ATTRIBUTES), "# preserved member attributes\n").unwrap();
    }
    let state = format!("{history}/state.json");
    assert_eq!(
        attribute_values(&repository, isolation, &state),
        ["set", "eska-test", "set", "UTF-16LE"]
    );
    TransportPlan::prepare(&imported.project)
        .unwrap()
        .publish()
        .unwrap();
    assert_eq!(
        attribute_values(&repository, isolation, &state),
        ["unset", "unset", "unset", "unspecified"]
    );
    Fixture {
        repository,
        project: imported.project,
        scope,
        accepted: imported.accepted,
        initial: imported.initial_ancestor.unwrap(),
    }
}

/// Commit only project authorities and raw history before cloning into a new checkout.
fn checkout(fixture: &Fixture, test: &TestDir, isolation: &Path, member: bool) -> PathBuf {
    let relative = if member { "projects/member [x]" } else { "." };
    if member {
        git(
            &fixture.repository,
            isolation,
            &["add", "--", ATTRIBUTES, "eska.toml", relative],
        );
    } else {
        git(
            &fixture.repository,
            isolation,
            &[
                "add",
                "--",
                ATTRIBUTES,
                "eska.toml",
                "src",
                ".eska/accepted",
                PROJECT_SOURCE_IMPORT_DIRECTORY,
            ],
        );
    }
    git(
        &fixture.repository,
        isolation,
        &["commit", "-m", "test raw ancestry transport"],
    );
    let checkout = test.0.join("checkout");
    git(
        &test.0,
        isolation,
        &[
            "clone",
            "--no-local",
            fixture.repository.to_str().unwrap(),
            checkout.to_str().unwrap(),
        ],
    );
    checkout.join(relative)
}

/// Both tracked initial and published incoming snapshots survive a clean clone with the same pins.
#[test]
fn clean_checkout_keeps_initial_ancestry_and_history_under_standalone_and_member_roots() {
    for member in [false, true] {
        let test = TestDir::new();
        let isolation = test.0.join("empty.attributes");
        fs::write(&isolation, b"").unwrap();
        let fixture = fixture(&test, &isolation, member);
        let project = &fixture.project;
        let ancestor = project_import_ancestor(project, &fixture.scope, &fixture.initial).unwrap();
        let incoming_source = test.0.join("incoming source");
        ancestor.restore(&incoming_source).unwrap();
        let owner = incoming_source.join("configuration.yaml");
        fs::write(
            &owner,
            fs::read_to_string(&owner)
                .unwrap()
                .replace("Accepted", "Incoming"),
        )
        .unwrap();
        let incoming_store = test.0.join("incoming store");
        let next = capture_import_ancestor(&incoming_source, &incoming_store).unwrap();
        let incoming = ImportAncestor::open(&incoming_store, &next).unwrap();
        prepare_project_source_import(
            project,
            &fixture.scope,
            &fixture.accepted,
            &ancestor,
            &incoming,
        )
        .unwrap()
        .publish()
        .unwrap();
        let source = EskaSource::open(project.clone(), fixture.scope.clone()).unwrap();
        let original = source
            .prepare(
                &fixture.accepted,
                &project.root().join(".eska/generations"),
                true,
            )
            .unwrap();
        let root = checkout(&fixture, &test, &isolation, member);
        assert_eq!(files(project.source()), files(&root.join("src")));
        assert_eq!(
            files(&project.root().join(".eska/accepted")),
            files(&root.join(".eska/accepted"))
        );
        assert_eq!(
            files(&project.root().join(PROJECT_SOURCE_IMPORT_DIRECTORY)),
            files(&root.join(PROJECT_SOURCE_IMPORT_DIRECTORY))
        );
        let copied = discover(&root).unwrap();
        let raw = project_import_ancestor(&copied, &fixture.scope, &next).unwrap();
        let restored = test.0.join("restored incoming");
        raw.restore(&restored).unwrap();
        assert_eq!(files(&restored), files(&incoming_source));
        ImportAncestor::open(
            &root.join(PROJECT_SOURCE_IMPORT_DIRECTORY).join("initial"),
            &fixture.initial,
        )
        .unwrap();
        let output = EskaSource::open(copied, fixture.scope.clone())
            .unwrap()
            .prepare(&fixture.accepted, &root.join(".eska/checked"), true)
            .unwrap();
        assert_eq!(files(original.xml()), files(output.xml()));
    }
}
