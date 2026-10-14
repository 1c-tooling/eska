use super::*;

mod ancestry;
use crate::{
    project_core::{
        ProjectConfiguration, ProjectType,
        config::ProjectConfig,
        discovery::discover,
        eska_source::EskaSource,
        metadata_model::ProjectScope,
        source_format::{AcceptedBase, AcceptedRef, accept_source, import_designer},
    },
    test_support::TestDir,
};
use std::{collections::BTreeMap, process::Command};

const SOURCE: &str = "источники [x]";
const BASE: &str = "# база [x]";

/// Select custom portable roots without creating native data or consulting Git.
fn project(root: &Path, source: &str, base: &str) -> Project {
    Project::new(
        root.to_owned(),
        root.join(source),
        ProjectConfiguration::new(
            ProjectType::Configuration,
            SourceFormat::Eska {
                accepted_base: SourcePath::parse(base).expect("portable base"),
            },
        ),
    )
    .expect("project paths")
}

#[test]
/// User attributes, BOM and CRLF survive verbatim; identical final rules are idempotent.
fn existing_attributes_are_preserved_and_publication_is_idempotent() {
    let test = TestDir::new();
    let project = project(&test.0, "источники [x]", "# база [x]");
    let original = "\u{feff}* text=auto\r\n# user rule\r\n*.mxl filter=lfs -text";
    fs::write(test.0.join(ATTRIBUTES), original).expect("user attributes");
    let plan = TransportPlan::prepare(&project).expect("transport preview");
    assert!(plan.updated().starts_with(original));
    assert!(plan.updated().contains("\"/источники \\\\[x\\\\]/**\""));
    assert!(plan.updated().contains("\"/# база \\\\[x\\\\]/**\""));
    assert!(!plan.updated().replace("\r\n", "").contains('\n'));
    assert_eq!(
        fs::read_to_string(test.0.join(ATTRIBUTES)).expect("unchanged"),
        original
    );
    plan.publish().expect("publish transport");
    let next = TransportPlan::prepare(&project).expect("repeat preparation");
    assert!(next.is_empty());
    assert_eq!(next.updated(), plan.updated());
    next.publish().expect("no-op publication");
    assert_eq!(
        fs::read_to_string(test.0.join(ATTRIBUTES)).expect("published"),
        plan.updated()
    );
}

#[test]
/// A preview never replaces another writer's new or modified attributes file.
fn new_and_existing_attributes_detect_concurrent_changes() {
    let test = TestDir::new();
    let project = project(&test.0, "src", "accepted");
    let first = TransportPlan::prepare(&project).expect("new attributes");
    assert!(!test.0.join(ATTRIBUTES).exists());
    fs::write(test.0.join(ATTRIBUTES), "# concurrent creation\n").expect("other writer");
    assert_eq!(
        first.publish().expect_err("creation conflict").code,
        "transport_conflict"
    );
    let next = TransportPlan::prepare(&project).expect("existing attributes");
    fs::write(test.0.join(ATTRIBUTES), "# concurrent modification\n").expect("other writer");
    assert_eq!(
        next.publish().expect_err("snapshot conflict").code,
        "transport_conflict"
    );
    assert_eq!(
        fs::read_to_string(test.0.join(ATTRIBUTES)).expect("preserved"),
        "# concurrent modification\n"
    );
    fs::remove_file(test.0.join(ATTRIBUTES)).expect("remove own attributes");
    TransportPlan::prepare(&project)
        .expect("fresh plan")
        .publish()
        .expect("create complete file");
    assert_eq!(
        fs::read_to_string(test.0.join(ATTRIBUTES)).expect("new file"),
        first.updated()
    );
    assert_eq!(fs::read_dir(&test.0).expect("root").count(), 1);
}

#[test]
/// Refuse nonportable roots and malformed files before any output is published.
fn transport_checks_format_layout_encoding_and_allocation_bound() {
    let test = TestDir::new();
    let native = Project::new(
        test.0.clone(),
        test.0.join("src"),
        ProjectConfiguration::new(ProjectType::Configuration, SourceFormat::DesignerXml),
    )
    .expect("native project");
    assert_eq!(
        TransportPlan::prepare(&native).expect_err("format").code,
        "source_format_mismatch"
    );
    for source in [".", "accepted", "accepted/nested", "src*"] {
        assert!(TransportPlan::prepare(&project(&test.0, source, "accepted")).is_err());
    }
    let project = project(&test.0, "src", "accepted");
    fs::write(test.0.join(ATTRIBUTES), [0xff]).expect("invalid UTF-8");
    assert_eq!(
        TransportPlan::prepare(&project).expect_err("encoding").code,
        "transport_encoding_unsupported"
    );
    fs::write(
        test.0.join(ATTRIBUTES),
        vec![b'#'; usize::try_from(MAX_ATTRIBUTES_BYTES).expect("test bound") + 1],
    )
    .expect("large attributes");
    assert_eq!(
        TransportPlan::prepare(&project).expect_err("bound").code,
        "transport_attributes_too_large"
    );
    fs::remove_file(test.0.join(ATTRIBUTES)).expect("remove own file");
    fs::create_dir(test.0.join(ATTRIBUTES)).expect("directory at attributes path");
    assert_eq!(
        TransportPlan::prepare(&project)
            .expect_err("unsafe entry")
            .code,
        "transport_unsafe_path"
    );
}

#[cfg(unix)]
#[test]
/// Neither preview nor publication follows a symlink to attributes outside the selected root.
fn aliases_are_refused_without_changing_the_target() {
    use std::os::unix::fs::symlink;
    let test = TestDir::new();
    let project = project(&test.0, "src", "accepted");
    let plan = TransportPlan::prepare(&project).expect("new attributes");
    let target = test.0.join("other");
    fs::write(&target, b"original target").expect("target");
    symlink(&target, test.0.join(ATTRIBUTES)).expect("alias");
    assert_eq!(
        TransportPlan::prepare(&project)
            .expect_err("preview alias")
            .code,
        "transport_unsafe_path"
    );
    assert_eq!(
        plan.publish().expect_err("concurrent alias").code,
        "transport_conflict"
    );
    assert_eq!(
        fs::read(target).expect("untouched target"),
        b"original target"
    );
}

/// Run Git only as a test oracle with isolated identity, attributes and system configuration.
fn git(root: &Path, isolation: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", isolation)
        .env_remove("GIT_INDEX_FILE")
        .args([
            "-c",
            "core.autocrlf=true",
            "-c",
            "user.name=ESKA Test",
            "-c",
            "user.email=eska-test@example.invalid",
            "-c",
        ])
        .arg(format!("core.attributesFile={}", isolation.display()))
        .args(args)
        .output()
        .expect("Git contract oracle");
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// Hash-independent byte inventory compares checkout inputs without imposing empty directories.
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(root.join(&parent)).expect("input directory") {
            let entry = entry.expect("input entry");
            let relative = parent.join(entry.file_name());
            if entry.file_type().expect("entry type").is_dir() {
                pending.push(relative);
            } else {
                result.insert(relative, fs::read(entry.path()).expect("input bytes"));
            }
        }
    }
    result
}

/// Build a real Git transport fixture with native empty directories and harmful input rules.
fn git_fixture(origin: &Path, isolation: &Path) -> (Project, AcceptedRef) {
    fs::create_dir(origin).expect("origin");
    let native = origin.join("native");
    fs::create_dir(&native).expect("native input");
    fs::write(native.join("Configuration.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name><Comment>Accepted</Comment></Properties></Configuration></MetaDataObject>"
    ).expect("configuration");
    fs::write(
        native.join("Module.bsl"),
        b"\xef\xbb\xbf// $Id: native $\r\nProcedure Sample()\r\nEndProcedure\r\n",
    )
    .expect("BSL with BOM and CRLF");
    fs::write(native.join("Resource.mxl"), [0, 0xff, 0x80, 10, 13]).expect("binary resource");
    fs::create_dir(native.join("Empty")).expect("empty native directory");
    import_designer(&native, &origin.join(SOURCE)).expect("source import");
    let pin = accept_source(&origin.join(SOURCE), &origin.join(BASE), &[]).expect("base");
    let config = ProjectConfig::new(ProjectType::Configuration)
        .with_source(PathBuf::from(SOURCE))
        .expect("source path")
        .with_source_format(SourceFormat::Eska {
            accepted_base: SourcePath::parse(BASE).expect("base path"),
        })
        .expect("source format");
    fs::write(origin.join("eska.toml"), config.to_toml().expect("TOML")).expect("manifest");
    fs::write(origin.join(ATTRIBUTES), format!(
        "* text=auto\n*.mxl filter=eska-test -text\n{} text ident filter=eska-test working-tree-encoding=UTF-16LE\n{} text ident filter=eska-test working-tree-encoding=UTF-16LE\n",
        quoted_pattern(&SourcePath::parse(SOURCE).expect("source pattern")),
        quoted_pattern(&SourcePath::parse(BASE).expect("base pattern")),
    )).expect("prior attributes");
    git(origin, isolation, &["init", "--initial-branch=main"]);
    git(
        origin,
        isolation,
        &[
            "config",
            "filter.eska-test.clean",
            "git hash-object --stdin",
        ],
    );
    assert_eq!(
        attribute_values(origin, isolation, &format!("{SOURCE}/Module.bsl")),
        ["set", "eska-test", "set", "UTF-16LE"]
    );
    (discover(origin).expect("configured project"), pin)
}

/// Decode Git's NUL records independently of human locale and paths containing whitespace.
fn attribute_values(root: &Path, isolation: &Path, path: &str) -> Vec<String> {
    let values = git(
        root,
        isolation,
        &[
            "check-attr",
            "-z",
            "text",
            "filter",
            "ident",
            "working-tree-encoding",
            "--",
            path,
        ],
    );
    let fields: Vec<_> = values
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .collect();
    assert_eq!(fields.len(), 12);
    fields
        .as_chunks::<3>()
        .0
        .iter()
        .map(|entry| String::from_utf8(entry[2].to_vec()).expect("attribute value"))
        .collect()
}

#[test]
/// A clean Git clone retains pinned blobs, source bytes and native output despite default filters.
fn clean_checkout_preserves_inputs_and_prepared_identity() {
    let test = TestDir::new();
    let isolation = test.0.join("empty.attributes");
    fs::write(&isolation, b"").expect("isolated Git settings");
    let origin = test.0.join("origin");
    let (project, pin) = git_fixture(&origin, &isolation);
    TransportPlan::prepare(&project)
        .expect("transport")
        .publish()
        .expect("attributes");
    let original = EskaSource::open(project, ProjectScope::Standalone).expect("source");
    let prepared = original
        .prepare(&pin, &origin.join("generations"), true)
        .expect("original generation");
    git(
        &origin,
        &isolation,
        &["add", "--", ATTRIBUTES, "eska.toml", SOURCE, BASE],
    );
    git(&origin, &isolation, &["commit", "-m", "test transport"]);
    let blob = format!("HEAD:{SOURCE}/Module.bsl");
    assert_eq!(
        git(&origin, &isolation, &["show", &blob]),
        fs::read(origin.join("native/Module.bsl")).expect("native BSL")
    );
    assert_eq!(
        attribute_values(&origin, &isolation, &format!("{SOURCE}/Module.bsl")),
        ["unset", "unset", "unset", "unspecified"]
    );
    git(
        &test.0,
        &isolation,
        &["clone", "--no-local", "origin", "checkout"],
    );
    let checkout = test.0.join("checkout");
    assert_eq!(files(&origin.join(SOURCE)), files(&checkout.join(SOURCE)));
    assert_eq!(files(&origin.join(BASE)), files(&checkout.join(BASE)));
    assert!(!checkout.join(SOURCE).join("Empty").exists());
    AcceptedBase::open(&checkout.join(BASE), &pin).expect("unchanged independent pin");
    let restored = EskaSource::open(
        discover(&checkout).expect("checkout project"),
        ProjectScope::Standalone,
    )
    .expect("checkout source");
    let output = restored
        .prepare(&pin, &checkout.join("generations"), true)
        .expect("checkout generation");
    assert_eq!(prepared.generation(), output.generation());
    assert_eq!(
        prepared.source_map().checksum().expect("map"),
        output.source_map().checksum().expect("map")
    );
    assert_eq!(files(prepared.xml()), files(output.xml()));
    assert!(output.xml().join("Empty").is_dir());
}

#[test]
/// Higher-priority Git policy is not silently rewritten; corrupted checkout blobs are refused.
fn info_attributes_override_is_visible_and_cannot_bypass_the_base_pin() {
    let test = TestDir::new();
    let isolation = test.0.join("empty.attributes");
    fs::write(&isolation, b"").expect("Git isolation");
    let origin = test.0.join("origin");
    let (project, pin) = git_fixture(&origin, &isolation);
    TransportPlan::prepare(&project)
        .expect("transport")
        .publish()
        .expect("attributes");
    fs::write(
        origin.join(".git/info/attributes"),
        format!(
            "{} filter=eska-test\n",
            quoted_pattern(&SourcePath::parse(BASE).expect("base pattern"))
        ),
    )
    .expect("higher-priority filter");
    assert_eq!(
        attribute_values(&origin, &isolation, &format!("{BASE}/accepted.json"))[1],
        "eska-test"
    );
    git(
        &origin,
        &isolation,
        &["add", "--", ATTRIBUTES, "eska.toml", SOURCE, BASE],
    );
    git(
        &origin,
        &isolation,
        &["commit", "-m", "corrupted transport control"],
    );
    git(
        &test.0,
        &isolation,
        &["clone", "--no-local", "origin", "checkout"],
    );
    let checkout = test.0.join("checkout");
    let source = EskaSource::open(
        discover(&checkout).expect("project"),
        ProjectScope::Standalone,
    )
    .expect("source bytes");
    assert!(
        source
            .prepare(&pin, &checkout.join("generations"), true)
            .is_err()
    );
    assert!(!checkout.join("generations").exists());
    assert!(
        fs::read_to_string(origin.join(".git/info/attributes"))
            .expect("untouched override")
            .contains("filter=eska-test")
    );
}
