//! Admission uses real ESKA source readers and the pinned TOML/topology APIs without a kernel host.

use std::{collections::BTreeMap, fs, path::PathBuf};

use eska::project_core::{
    ProjectType,
    source_format::{SourcePath, SourceReader, import_designer_queries},
};
use ide_db::metadata::RootKind;
use project_model::{ProjectConfig, TopologyError};

use crate::session::tests::support::TestDirectory;

use super::*;

struct Fixture {
    directory: TestDirectory,
    sources: BTreeMap<String, PathBuf>,
}

impl Fixture {
    /// The small native inputs are private producers of actual reversible CF/CFE source profiles.
    fn new() -> Self {
        let mut fixture = Self {
            directory: TestDirectory::new(),
            sources: BTreeMap::new(),
        };
        for (name, kind, uuid) in [
            ("base", ProjectType::Configuration, 1),
            ("A", ProjectType::Extension, 2),
            ("B", ProjectType::Extension, 3),
            ("C", ProjectType::Extension, 4),
        ] {
            fixture.add(name, kind, uuid);
        }
        fixture
    }

    /// Each imported root is isolated from both the repository and every other test directory.
    fn add(&mut self, name: &str, kind: ProjectType, uuid: u32) {
        let native = self.directory.0.join(format!("native-{name}"));
        fs::create_dir(&native).unwrap();
        let tag = match kind {
            ProjectType::Configuration | ProjectType::Extension => "Configuration",
            ProjectType::Processing => "ExternalDataProcessor",
            ProjectType::Report => "ExternalReport",
        };
        let purpose = if kind == ProjectType::Extension {
            "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>"
        } else {
            ""
        };
        let text = format!(
            "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{tag} uuid=\"10000000-0000-4000-8000-{uuid:012x}\"><Properties><Name>{name}</Name>{purpose}</Properties><ChildObjects/></{tag}></MetaDataObject>"
        );
        fs::write(native.join(format!("{tag}.xml")), text).unwrap();
        let source = self.directory.0.join(name);
        import_designer_queries(&native, &source).unwrap();
        self.sources.insert(name.into(), source);
    }

    /// TOML is parsed by the actual pinned contract, including dependency aliases and source options.
    fn config(&self, source: &str) -> ProjectConfig {
        let path = self.directory.0.join("bsl-analyzer.toml");
        fs::write(&path, format!("[source]\n{source}\n")).unwrap();
        ProjectConfig::load_from_file(&path).unwrap()
    }

    /// Supplied reader order is deliberately unrelated to declaration order.
    fn readers(&self, names: &[&str]) -> Vec<SourceReader> {
        names
            .iter()
            .map(|name| SourceReader::open(&self.sources[*name]).unwrap())
            .collect()
    }

    /// The selected root is explicit and cannot be replaced by a discovered or inferred base.
    fn admit(
        &self,
        config: &ProjectConfig,
        names: &[&str],
        selected: &str,
    ) -> Result<AdmittedComposition, AdmissionError> {
        admit(
            &self.directory.0,
            config,
            self.readers(names),
            &self.sources[selected],
        )
    }
}

/// Dependency visibility follows the normalized topology, independently of reader or declaration order.
#[test]
fn explicit_topology_retains_roles_exact_headers_and_dependency_closures() {
    let fixture = Fixture::new();
    let config = fixture.config("root = \"base\"\nextensions = [{name=\"B\", path=\"B\", dependsOn=[\"A\"]}, {name=\"A\", path=\"A\"}, {name=\"C\", path=\"C\"}]");
    let composition = fixture
        .admit(&config, &["C", "A", "base", "B"], "B")
        .unwrap();
    assert_eq!(composition.selected().index(), 1);
    assert_eq!(
        composition.snapshot().kinds,
        [
            RootKind::Base,
            RootKind::Extension,
            RootKind::Extension,
            RootKind::Extension
        ]
    );
    assert_eq!(
        composition.snapshot().closures,
        [vec![], vec![2], vec![], vec![]]
    );
    assert_eq!(composition.snapshot().topological_order, [0, 2, 1, 3]);
    assert!(composition.snapshot().has_base());
    assert_eq!(
        composition.snapshot().fingerprint.as_ref().unwrap().len(),
        64
    );
    assert_eq!(composition.source_sha256().len(), 64);
    for (index, (root, name)) in composition
        .roots()
        .iter()
        .zip(["base", "B", "A", "C"])
        .enumerate()
    {
        assert_eq!(root.id().index(), u32::try_from(index).unwrap());
        assert_eq!(root.reader().root(), fixture.sources[name]);
        assert_eq!(root.reader().configuration().declarations[0].name, name);
    }
    assert_eq!(
        composition.roots()[0].reader().configuration().declarations[0]
            .uuid
            .as_str(),
        "10000000-0000-4000-8000-000000000001"
    );
    assert_eq!(
        composition.roots()[1].reader().configuration().declarations[0]
            .uuid
            .as_str(),
        "10000000-0000-4000-8000-000000000003"
    );
}

/// A selected CF preserves zero-config admission; a standalone CFE cannot invent an owning base.
#[test]
fn default_admission_preserves_cf_and_refuses_uncomposed_extensions() {
    let fixture = Fixture::new();
    let composition = fixture
        .admit(&ProjectConfig::default(), &["base"], "base")
        .unwrap();
    assert_eq!(composition.roots().len(), 1);
    assert_eq!(composition.selected().index(), 0);
    assert_eq!(composition.snapshot().kinds, [RootKind::Base]);
    let (selected, mut roots, snapshot, digest) = composition.into_parts();
    assert_eq!(selected.index(), 0);
    assert_eq!(snapshot.paths.len(), 1);
    assert_eq!(digest.len(), 64);
    let root = roots.pop().unwrap();
    assert_eq!(root.kind(), RootKind::Base);
    assert_eq!(root.id(), selected);
    assert_eq!(root.into_reader().root(), fixture.sources["base"]);
    assert!(matches!(
        fixture.admit(&ProjectConfig::default(), &["A"], "A"),
        Err(AdmissionError::MissingBase)
    ));
}

/// The real topology validator refuses every ambiguous or cyclic dependency before publication.
#[test]
fn malformed_dependency_graphs_are_fail_closed() {
    let fixture = Fixture::new();
    for (entries, expected) in [
        ("[{name='A',path='A',dependsOn=['Missing']}]", "unknown"),
        ("[{name='A',path='A',dependsOn=['a']}]", "self"),
        (
            "[{name='A',path='A',dependsOn=['B','b']},{name='B',path='B'}]",
            "edge",
        ),
        (
            "[{name='A',path='A',dependsOn=['B']},{name='B',path='B',dependsOn=['A']}]",
            "cycle",
        ),
        ("[{name='A',path='A'},{name='a',path='B'}]", "name"),
    ] {
        let config = fixture.config(&format!("root='base'\nextensions={entries}"));
        let names: &[&str] = if entries.contains("path='B'") {
            &["base", "A", "B"]
        } else {
            &["base", "A"]
        };
        let error = fixture.admit(&config, names, "A").err().unwrap();
        assert!(
            matches!(
                (&error, expected),
                (
                    AdmissionError::Topology(TopologyError::UnknownDependency { .. }),
                    "unknown"
                ) | (
                    AdmissionError::Topology(TopologyError::SelfReference { .. }),
                    "self"
                ) | (
                    AdmissionError::Topology(TopologyError::DuplicateEdge { .. }),
                    "edge"
                ) | (
                    AdmissionError::Topology(TopologyError::Cycle { .. }),
                    "cycle"
                ) | (
                    AdmissionError::Topology(TopologyError::DuplicateName { .. }),
                    "name"
                )
            ),
            "{error:?}"
        );
    }
}

/// Native files cannot override ESKA roles, and path or root-UUID aliases cannot produce duplicate slots.
#[test]
fn declared_root_roles_paths_and_header_uuid_are_exact() {
    let mut fixture = Fixture::new();
    fs::write(
        fixture.sources["base"].join("Configuration.xml"),
        "unrelated native bytes",
    )
    .unwrap();
    let config = fixture.config("root='A'\nextensions=[]");
    assert!(matches!(
        fixture.admit(&config, &["A"], "A"),
        Err(AdmissionError::RoleMismatch {
            expected: ProjectType::Configuration,
            actual: ProjectType::Extension,
            ..
        })
    ));
    let config = fixture.config("root='base'\nextensions=[{name='Alias',path='base'}]");
    assert!(matches!(
        fixture.admit(&config, &["base"], "base"),
        Err(AdmissionError::DuplicatePath(_))
    ));
    fixture.add("same-uuid", ProjectType::Extension, 1);
    let config = fixture.config("root='base'\nextensions=[{name='Same',path='same-uuid'}]");
    assert!(matches!(
        fixture.admit(&config, &["base", "same-uuid"], "base"),
        Err(AdmissionError::DuplicateHeaderUuid(_))
    ));
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let name = if kind == ProjectType::Processing {
            "processing"
        } else {
            "report"
        };
        fixture.add(name, kind, 9);
        let config = fixture.config(&format!(
            "root='base'\nextensions=[{{name='Wrong',path='{name}'}}]"
        ));
        assert!(
            matches!(fixture.admit(&config, &["base", name], name), Err(AdmissionError::RoleMismatch { expected: ProjectType::Extension, actual, .. }) if actual == kind)
        );
    }
}

/// Unknown supplied inputs and unsupported discovery or exclusion shapes never silently narrow the context.
#[test]
fn incomplete_or_unsupported_source_sets_refuse_admission() {
    let mut fixture = Fixture::new();
    let config = fixture.config("root='base'\nextensions=[{name='A',path='A'}]");
    assert!(matches!(
        fixture.admit(&config, &["base"], "base"),
        Err(AdmissionError::MissingReader(_))
    ));
    assert!(matches!(
        fixture.admit(&config, &["base", "A", "C"], "base"),
        Err(AdmissionError::UndeclaredReader(_))
    ));
    assert!(matches!(
        fixture.admit(&config, &["base", "A"], "C"),
        Err(AdmissionError::MissingSelectedRoot)
    ));
    assert!(matches!(
        fixture.admit(&config, &["base", "base", "A"], "base"),
        Err(AdmissionError::DuplicatePath(_))
    ));
    let missing = fixture.config("root='base'\nextensions=[{name='Missing',path='Missing'}]");
    assert!(matches!(
        fixture.admit(&missing, &["base"], "base"),
        Err(AdmissionError::PathUnavailable(_))
    ));
    let nested = fixture.sources["base"].join("nested");
    import_designer_queries(&fixture.directory.0.join("native-A"), &nested).unwrap();
    fixture.sources.insert("nested".into(), nested);
    let overlap = fixture.config("root='base'\nextensions=[{name='Nested',path='base/nested'}]");
    assert!(matches!(
        fixture.admit(&overlap, &["base", "nested"], "base"),
        Err(AdmissionError::OverlappingRoots(_, _))
    ));
    for source in [
        "root='base'\nextensions=['A']",
        "root='base'\nextensions=[{name='A',path='*'}]",
        "root='base'\nexclude=['C']",
        "root='base'\nexternals=[{name='E',path='C'}]",
        "configuration={id='Shared',version='1'}",
    ] {
        assert!(matches!(
            fixture.admit(&fixture.config(source), &["base"], "base"),
            Err(AdmissionError::UnsupportedSourceSet)
        ));
    }
}

/// Fresh readers refuse invalid current headers; exact source hashes complement topology identity.
#[test]
fn root_header_fingerprints_and_invalid_reopen_require_source_repair() {
    let fixture = Fixture::new();
    let config = fixture.config("root='base'\nextensions=[{name='A',path='A'}]");
    let original = fixture.admit(&config, &["base", "A"], "A").unwrap();
    let reader = SourceReader::open(&fixture.sources["A"]).unwrap();
    let path = reader.root().join(reader.configuration().path.as_path());
    let saved = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("{saved}\n")).unwrap();
    let changed = fixture.admit(&config, &["base", "A"], "A").unwrap();
    assert_eq!(
        changed.snapshot().fingerprint,
        original.snapshot().fingerprint
    );
    assert_ne!(changed.source_sha256(), original.source_sha256());
    fs::write(&path, "not: [valid").unwrap();
    assert!(SourceReader::open(&fixture.sources["A"]).is_err());
    fs::write(&path, saved).unwrap();
    let repaired = fixture.admit(&config, &["base", "A"], "A").unwrap();
    assert_eq!(repaired.source_sha256(), original.source_sha256());
}

/// Identical authoring paths retain separate root keys while descriptors, forms and bodies share file IDs.
#[test]
fn one_allocator_and_root_qualified_keys_do_not_synthesize_source_prefixes() {
    let fixture = Fixture::new();
    let config =
        fixture.config("root='base'\nextensions=[{name='A',path='A'},{name='B',path='B'}]");
    let composition = fixture.admit(&config, &["base", "A", "B"], "B").unwrap();
    let mut allocator = FileIdAllocator::default();
    let mut sources = BTreeMap::new();
    let mut files = Vec::new();
    for root in composition.roots() {
        let key = SourceKey {
            root: root.id(),
            source: SourcePath::parse("configuration.yaml").unwrap(),
        };
        assert_eq!(key.source.as_str(), "configuration.yaml");
        sources.insert(key, allocator.allocate().unwrap());
        // A nested descriptor and a BSL body follow their owner without resetting for another root.
        files.push(allocator.allocate().unwrap());
        files.push(allocator.allocate().unwrap());
    }
    assert_eq!(sources.len(), 3);
    assert_eq!(
        sources.values().map(|file| file.0).collect::<Vec<_>>(),
        [0, 3, 6]
    );
    assert_eq!(
        files.iter().map(|file| file.0).collect::<Vec<_>>(),
        [1, 2, 4, 5, 7, 8]
    );
}
