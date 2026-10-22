use std::{fs, path::PathBuf};

use crate::backend::project::{discovery, patch};

use crate::{
    support::TestDir,
    vcs::support::{git, repository},
};

/// Create and commit a minimal configuration with one eligible server common module.
fn project() -> (TestDir, PathBuf) {
    let fixture = repository();
    let root = fixture.0.join("project");
    fs::create_dir_all(root.join("src/CommonModules/Math/Ext")).expect("create source tree");
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'configuration'\n\n[build]\nplatform_version = '8.3.27.2325'\n\n[vcs.workflow]\npreset = 'trunk'\n",
    )
    .expect("write project configuration");
    fs::write(
        root.join("src/Configuration.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Configuration/></MetaDataObject>"#,
    )
    .expect("write configuration descriptor");
    fs::write(
        root.join("src/CommonModules/Math.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><CommonModule uuid="11111111-1111-1111-1111-111111111111"><Properties><Name>Math</Name><Global>false</Global><Server>true</Server><ClientManagedApplication>false</ClientManagedApplication><ClientOrdinaryApplication>false</ClientOrdinaryApplication><Privileged>false</Privileged><ReturnValuesReuse>DontUse</ReturnValuesReuse></Properties></CommonModule></MetaDataObject>"#,
    )
    .expect("write module descriptor");
    fs::write(
        root.join("src/CommonModules/Math/Ext/Module.bsl"),
        "Function AddOne(Value) Export\n    Return Value + 1;\nEndFunction\n",
    )
    .expect("write module");
    fs::write(root.join("src/opaque.bin"), b"\0\xff\r\n").expect("opaque source");
    git(&fixture.0, &["add", "--", "project"]);
    git(&fixture.0, &["commit", "-m", "base"]);
    git(&fixture.0, &["switch", "-c", "feature"]);
    (fixture, root)
}

/// Commit every path currently changed in the fixture.
fn commit(root: &std::path::Path, message: &str) {
    git(root, &["add", "--all"]);
    git(root, &["commit", "-m", message]);
}

/// Plan only changed method bodies from the immutable merge-base snapshot.
#[test]
fn plans_supported_committed_delta_and_ignores_unrelated_repository_paths() {
    let (fixture, root) = project();
    fs::write(
        root.join("src/CommonModules/Math/Ext/Module.bsl"),
        "Function AddOne(Value) Export\n    Return Value + 2;\nEndFunction\n",
    )
    .expect("change method body");
    fs::write(fixture.0.join("README.md"), "unrelated\n").expect("write unrelated path");
    commit(&fixture.0, "feature");

    let project = discovery::discover(&root).expect("discover project");
    let plan = patch::plan(&project, None).expect("supported patch plan");

    assert_eq!(plan.description.base, "refs/heads/main");
    assert_eq!(plan.description.modules.len(), 1);
    assert_eq!(plan.description.modules[0].name, "Math");
    assert_eq!(plan.description.modules[0].methods, ["AddOne"]);
    assert_eq!(plan.description.ignored_paths, ["README.md"]);
    assert_eq!(plan.description.changes.len(), 2);
    assert_eq!(plan.description.changes[0].decision, "ignored");
    assert_eq!(plan.description.changes[0].reason, "outside_project_source");
    assert_eq!(plan.description.changes[1].decision, "included");
    assert_eq!(plan.description.changes[1].reason, "changed_method_bodies");
    assert_ne!(plan.description.merge_base, plan.description.head);
}

/// Ignore comment-only edits because they do not change executable method tokens.
#[test]
fn produces_empty_plan_for_nonsemantic_method_edits() {
    let (fixture, root) = project();
    let path = root.join("src/CommonModules/Math/Ext/Module.bsl");
    let source = fs::read_to_string(&path).expect("read module");
    fs::write(
        &path,
        source.replace("Return", "// explanation\n    Return"),
    )
    .expect("add comment");
    commit(&fixture.0, "comment");

    let project = discovery::discover(&root).expect("discover project");
    let plan = patch::plan(&project, Some("main")).expect("empty patch plan");
    assert_eq!(plan.description.modules, []);
    assert_eq!(plan.description.changes[0].reason, "nonsemantic");
}

/// Prepared transfer data keeps the committed baseline exact and generated modules valid for 1C.
#[test]
fn materialization_preserves_baseline_payload_and_claims_only_empty_namespaces() {
    let (fixture, root) = project();
    let module = root.join("src/CommonModules/Math/Ext/Module.bsl");
    let original = fs::read(&module).unwrap();
    fs::write(
        &module,
        String::from_utf8(original.clone())
            .unwrap()
            .replace("+ 1", "+ 2"),
    )
    .unwrap();
    commit(&fixture.0, "feature");
    let project = discovery::discover(&root).unwrap();
    let plan = patch::plan(&project, None).unwrap();
    let target = TestDir::new();
    eska_vcs::project::patch::materialize(&plan, &target.0).unwrap();
    assert_eq!(
        fs::read(target.0.join("base/CommonModules/Math/Ext/Module.bsl")).unwrap(),
        original
    );
    assert_eq!(
        fs::read(target.0.join("base/opaque.bin")).unwrap(),
        b"\0\xff\r\n"
    );
    for (relative, multiline) in [
        ("modules/Math.xml", false),
        ("modules/Math/Ext/Module.bsl", true),
    ] {
        let bytes = fs::read(target.0.join(relative)).unwrap();
        assert!(bytes.starts_with(b"\xef\xbb\xbf"));
        assert!(!bytes[3..].windows(3).any(|part| part == b"\xef\xbb\xbf"));
        if multiline {
            assert!(bytes.windows(2).any(|part| part == b"\r\n"));
        }
        assert!(
            !String::from_utf8(bytes)
                .unwrap()
                .replace("\r\n", "")
                .contains('\n')
        );
    }
    let before = fs::read(target.0.join("modules/Math/Ext/Module.bsl")).unwrap();
    assert_eq!(
        eska_vcs::project::patch::materialize(&plan, &target.0)
            .unwrap_err()
            .code,
        "path"
    );
    assert_eq!(
        fs::read(target.0.join("modules/Math/Ext/Module.bsl")).unwrap(),
        before
    );
    assert!(
        String::from_utf8(fs::read(&module).unwrap())
            .unwrap()
            .contains("+ 2")
    );
}

/// Reject signature, descriptor and dirty-worktree inputs before platform execution.
#[test]
fn rejects_unproven_or_uncommitted_changes() {
    for (relative, replacement, code) in [
        (
            "src/CommonModules/Math/Ext/Module.bsl",
            "Function AddOne(Other) Export\n    Return Other + 1;\nEndFunction\n",
            "bsl",
        ),
        ("src/CommonModules/Math.xml", "<changed/>", "unsupported"),
    ] {
        let (fixture, root) = project();
        fs::write(root.join(relative), replacement).expect("change source");
        commit(&fixture.0, "unsupported");
        let project = discovery::discover(&root).expect("discover project");
        assert_eq!(patch::plan(&project, Some("main")).unwrap_err().code, code);
    }

    let (_fixture, root) = project();
    fs::write(
        root.join("src/CommonModules/Math/Ext/Module.bsl"),
        "dirty\n",
    )
    .expect("dirty module");
    let project = discovery::discover(&root).expect("discover project");
    assert_eq!(
        patch::plan(&project, Some("main")).unwrap_err().code,
        "dirty"
    );
}
