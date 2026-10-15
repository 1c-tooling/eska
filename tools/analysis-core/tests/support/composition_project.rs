//! Real reversible CF/CFE inputs exercise declared topology without native discovery or platform calls.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use eska::project_core::{
    ProjectName,
    creation::import_query_project,
    source_format::{SourcePath, SourceReader},
};

pub const CALLER: &str =
    "Procedure Run() Export\n    Value = Catalogs.Items.CreateItem();\n    Value.\nEndProcedure\n";
pub const CONFIG: &str = "[source]\nroot='../base/src'\nextensions=[{name='A',path='../a/src'}, {name='B',path='src',depends_on=['A']}, {name='Sibling',path='../sibling/src'}]\n\n[diagnostics.parameters]\n";

pub struct CompositionProject {
    pub roots: BTreeMap<&'static str, PathBuf>,
    pub settings: PathBuf,
    pub module: SourcePath,
    pub catalog: SourcePath,
    pub selected_accepted_sha256: String,
}

/// Each root is a separately owned project with its own real published source/import history.
pub fn project(directory: &Path) -> CompositionProject {
    let mut roots = BTreeMap::new();
    let mut selected_accepted_sha256 = String::new();
    for (index, (name, label)) in [
        ("base", "Base"),
        ("a", "A"),
        ("b", "B"),
        ("sibling", "Sibling"),
    ]
    .into_iter()
    .enumerate()
    {
        let native = directory.join(format!("native-{name}"));
        let common = native.join("CommonModules/Tools/Ext");
        fs::create_dir_all(&common).unwrap();
        let extension = if index == 0 {
            ""
        } else {
            "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>"
        };
        let catalog_child = if name == "b" {
            ""
        } else {
            "<Catalog>Items</Catalog>"
        };
        fs::write(native.join("Configuration.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-{:012x}\"><Properties><Name>{label}</Name><Version>1.0.0</Version>{extension}</Properties><ChildObjects><CommonModule>Tools</CommonModule>{catalog_child}</ChildObjects></Configuration></MetaDataObject>", index + 1)).unwrap();
        fs::write(native.join("CommonModules/Tools.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><CommonModule uuid=\"20000000-0000-4000-8000-{:012x}\"><Properties><Name>Tools</Name><Global>false</Global><Server>true</Server><ClientManagedApplication>false</ClientManagedApplication><ServerCall>true</ServerCall></Properties></CommonModule></MetaDataObject>", index + 1)).unwrap();
        let body = if name == "b" {
            CALLER.to_owned()
        } else {
            format!("Procedure {label}Method() Export\nEndProcedure\n")
        };
        fs::write(common.join("Module.bsl"), body).unwrap();
        if name != "b" {
            catalog(&native, name, index);
        }
        let root = directory.join(name);
        let imported = import_query_project(
            &native,
            &root,
            ProjectName::parse(name.into()).unwrap(),
            None,
            &[],
        )
        .unwrap();
        if name == "b" {
            selected_accepted_sha256 = imported.accepted.sha256;
        }
        roots.insert(name, root);
    }
    let settings = roots["b"].join("bsl-analyzer.toml");
    fs::write(&settings, CONFIG).unwrap();
    let selected = SourceReader::open(&roots["b"].join("src")).unwrap();
    let module = selected.module_paths().next().unwrap().clone();
    let base = SourceReader::open(&roots["base"].join("src")).unwrap();
    let catalog = base
        .owner_for_native_main(&SourcePath::parse("Catalogs/Items.xml").unwrap())
        .unwrap()
        .clone();
    CompositionProject {
        roots,
        settings,
        module,
        catalog,
        selected_accepted_sha256,
    }
}

/// Adoption points at the base's real UUID; a same-named sibling owns a separate object.
fn catalog(native: &Path, name: &str, index: usize) {
    fs::create_dir_all(native.join("Catalogs")).unwrap();
    let (field, ownership) = match name {
        "base" => ("BaseField", ""),
        "a" => (
            "AField",
            "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>30000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>",
        ),
        _ => ("SiblingField", "<ObjectBelonging>Own</ObjectBelonging>"),
    };
    fs::write(native.join("Catalogs/Items.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><Catalog uuid=\"30000000-0000-4000-8000-{:012x}\"><Properties><Name>Items</Name><CodeLength>9</CodeLength><DescriptionLength>100</DescriptionLength>{ownership}</Properties><ChildObjects><Attribute uuid=\"40000000-0000-4000-8000-{:012x}\"><Properties><Name>{field}</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>", index + 1, index + 1)).unwrap();
}

/// Native parse-only rule policy keeps producer/composition acceptance independent of unrelated rules.
pub fn parse_only(project: &CompositionProject) {
    use std::fmt::Write as _;
    let mut settings = CONFIG.to_owned();
    for code in ide::all_diagnostic_codes() {
        writeln!(
            &mut settings,
            "{}={}",
            code.as_str(),
            code == ide::DiagnosticCode::ParseError
        )
        .unwrap();
    }
    fs::write(&project.settings, settings).unwrap();
}
