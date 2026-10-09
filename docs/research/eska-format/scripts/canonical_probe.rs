//! Exercise tracked canonical metadata inputs in one resident host, without XML export.

use std::{collections::BTreeSet, error::Error, fs, path::Path, sync::Arc, time::Instant};

use base_db::{Locale, SourceDatabase, SourceRootId};
use bsl_metadata::{Attribute, AttributeType, Configuration, MdoType, MetadataObject};
use eska::{
    analysis_core::{InputRequest, InputState, MetadataInputs, OwnerInput},
    project_core::{
        MetadataKind,
        source_format::{MetadataSnapshot, SourcePath, SourceReader, import_designer},
    },
};
use ide_db::{
    GraphConfigCache,
    metadata::{
        CanonicalMdoInput, CanonicalMdoInputs, MdoEntry, MetadataListingData, config_index,
        resolve_metadata_object,
    },
};
use ide_host_core::{AnalysisHost, FileTextSource, build_source_root, set_file_text_source};
use salsa::Setter;
use serde_json::json;
use vfs::FileId;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const CODE: &str = "// Demonstrates tracked canonical metadata.\nПроцедура Probe()\n    Item = Справочники.Sample.СоздатьЭлемент();\n    Item.x = 0;\nКонецПроцедуры\n";

/// Keep the same bounded projection as EF-17 so this measures input plumbing, not type parity.
fn catalog(snapshot: &MetadataSnapshot) -> Result<MetadataObject> {
    let document = snapshot.documents.first().ok_or("missing descriptor")?;
    let main = &document
        .descriptor
        .objects
        .first()
        .ok_or("missing catalog")?
        .metadata;
    if main.kind() != MetadataKind::Catalog {
        return Err("unsupported canonical object".into());
    }
    let mut object = MetadataObject::new(MdoType::Catalog, main.name());
    object.uuid = Some(main.uuid().parse()?);
    for child in &document.descriptor.objects {
        if child.metadata.kind() == MetadataKind::Attribute
            && child.metadata.parent() == Some(main.id())
        {
            object.attributes.push(Attribute {
                name: child.metadata.name().into(),
                name_en: None,
                attr_type: AttributeType::Unknown,
            });
        }
    }
    Ok(object)
}

/// Query a real BSL completion, using the current host revision.
fn labels(host: &AnalysisHost, root: &Path) -> Result<BTreeSet<String>> {
    let offset = CODE.find("Item.x").ok_or("missing cursor")? + "Item.".len();
    Ok(host
        .analysis()
        .completions(
            FileId(0),
            offset.try_into()?,
            Some(root.to_path_buf()),
            Locale::Ru,
        )
        .into_iter()
        .map(|item| item.label)
        .collect())
}

/// Replace only a parsed Name scalar; the shared input controller validates the whole draft.
fn draft(
    reader: &SourceReader,
    path: &SourcePath,
    text: &str,
    field: &str,
    replacement: &str,
) -> Result<String> {
    let saved = reader.read_owner(path)?;
    let declaration = saved
        .declarations
        .iter()
        .find(|item| item.kind == MetadataKind::Attribute && item.name == field)
        .ok_or("missing field")?;
    let mut text = text.to_owned();
    text.replace_range(declaration.name_range.clone(), replacement);
    Ok(text)
}

/// Compare tracked query execution counts, rather than inferring invalidation from elapsed time.
fn executions(host: &AnalysisHost, family: &str) -> Result<u64> {
    host.raw_database()
        .salsa_family_churn(family)
        .map(|counts| counts.0)
        .ok_or_else(|| "Salsa events unavailable".into())
}

/// Claim a tiny native fixture; no existing input or destination is overwritten.
fn fixture(root: &Path) -> Result<()> {
    fs::create_dir(root)?;
    fs::create_dir(root.join("Catalogs"))?;
    let header = "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\">";
    let configuration = format!(
        "{header}<Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>CanonicalProbe</Name></Properties><ChildObjects><Catalog>Sample</Catalog><Catalog>Other</Catalog></ChildObjects></Configuration></MetaDataObject>\r\n"
    );
    fs::write(root.join("Configuration.xml"), configuration)?;
    for (name, field, suffix) in [("Sample", "SourceField", "1"), ("Other", "OtherField", "2")] {
        let text = format!(
            "{header}<Catalog uuid=\"20000000-0000-4000-8000-00000000000{suffix}\"><Properties><Name>{name}</Name></Properties><ChildObjects><Attribute uuid=\"30000000-0000-4000-8000-00000000000{suffix}\"><Properties><Name>{field}</Name></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>\r\n"
        );
        fs::write(root.join("Catalogs").join(format!("{name}.xml")), text)?;
    }
    Ok(())
}

/// Update one canonical input outside all queries; pending values cannot fall back to XML.
fn update(host: &mut AnalysisHost, input: CanonicalMdoInput, owner: &OwnerInput) -> Result<()> {
    let object = match &owner.state {
        InputState::Ready(snapshot) => Some(Arc::new(catalog(snapshot)?)),
        InputState::Pending | InputState::Unavailable(_) => None,
    };
    let revision = match &owner.source_sha256 {
        Some(hash) => hash.clone(),
        None => serde_json::to_string(&owner.revision)?,
    };
    let database = host.raw_database_mut();
    input.set_value(database).to(object);
    input.set_source_revision(database).to(revision);
    Ok(())
}

/// Run saved, draft, unrelated, invalid and restored revisions in one database.
fn main() -> Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .ok_or("expected NEW_DEMO_DIRECTORY")?;
    let output = Path::new(&root);
    fs::create_dir(output)?;
    let native = output.join("native");
    fixture(&native)?;
    let source = output.join("source");
    import_designer(&native, &source)?;
    let source = fs::canonicalize(source)?;
    let reader = SourceReader::open(&source)?;
    let mut metadata = MetadataInputs::open(&source)?;
    let request = InputRequest::default();
    let sample_path = SourcePath::parse("Catalogs/Sample/object.yaml")?;
    let other_path = SourcePath::parse("Catalogs/Other/object.yaml")?;
    for path in metadata.owner_paths().cloned().collect::<Vec<_>>() {
        metadata
            .load_saved(&path, metadata.revision(), &request)
            .map_err(|error| format!("{error:?}"))?;
    }
    let sample_text = fs::read_to_string(source.join(sample_path.as_str()))?;
    let other_text = fs::read_to_string(source.join(other_path.as_str()))?;
    let mut host = AnalysisHost::default();
    let database = host.raw_database_mut();
    // Fallbacks for unsupported metadata families get an explicit empty model, never disk XML.
    let cache = Arc::new(GraphConfigCache::new());
    cache.insert(
        source.clone(),
        Arc::new(Configuration::new("CanonicalProbe")),
    );
    database.set_graph_config_cache(cache);
    database.set_all_config_paths(vec![(None, source.clone())]);
    database.set_workspace_load_complete(true);
    let files = [
        (FileId(0), source.join("CommonModules/Probe/Ext/Module.bsl")),
        (FileId(1), source.join(sample_path.as_str())),
        (FileId(2), source.join(other_path.as_str())),
    ];
    database.set_source_root(SourceRootId(0), build_source_root(&files));
    for (file_id, _) in &files {
        database.set_file_source_root(*file_id, SourceRootId(0));
    }
    set_file_text_source(database, FileId(0), FileTextSource::Overlay(CODE));
    // If the resolver accidentally parses these as XML it cannot find the catalog fields.
    set_file_text_source(database, FileId(1), FileTextSource::Overlay(&sample_text));
    set_file_text_source(database, FileId(2), FileTextSource::Overlay(&other_text));
    database.set_metadata_listing(
        source.to_str().ok_or("non UTF-8 path")?,
        MetadataListingData {
            entries: vec![
                MdoEntry {
                    kind: MdoType::Catalog,
                    name: "Sample".into(),
                    main: FileId(1),
                    predefined: None,
                },
                MdoEntry {
                    kind: MdoType::Catalog,
                    name: "Other".into(),
                    main: FileId(2),
                    predefined: None,
                },
            ],
            ..MetadataListingData::default()
        },
    );
    let listing = database
        .metadata_listing(source.to_str().ok_or("non UTF-8 path")?)
        .ok_or("missing listing")?;
    let sample_input = CanonicalMdoInput::new(database, None, "pending".into());
    let other_input = CanonicalMdoInput::new(database, None, "pending".into());
    let inputs: CanonicalMdoInputs = [(FileId(1), sample_input), (FileId(2), other_input)]
        .into_iter()
        .collect();
    listing.set_canonical_mdos(database).to(Arc::new(inputs));
    let index = Arc::clone(config_index(database, listing));
    update(
        &mut host,
        sample_input,
        metadata.owner(&sample_path).ok_or("missing input")?,
    )?;
    update(
        &mut host,
        other_input,
        metadata.owner(&other_path).ok_or("missing input")?,
    )?;
    if !labels(&host, &source)?.contains("SourceField") {
        return Err("saved completion missing".into());
    }
    let sibling = resolve_metadata_object(
        host.raw_database(),
        listing,
        MdoType::Catalog,
        "Other".into(),
    )
    .ok_or("missing sibling")?;
    let sibling_executions = executions(&host, "resolve_metadata_object")?;
    if sibling_executions < 2 {
        return Err("object resolution was not observed".into());
    }
    let started = Instant::now();
    let changed = draft(
        &reader,
        &sample_path,
        &sample_text,
        "SourceField",
        "DraftField",
    )?;
    metadata
        .apply_overlay(&sample_path, metadata.revision(), 1, &changed, &request)
        .map_err(|error| format!("{error:?}"))?;
    update(
        &mut host,
        sample_input,
        metadata.owner(&sample_path).ok_or("missing input")?,
    )?;
    let sibling_again = resolve_metadata_object(
        host.raw_database(),
        listing,
        MdoType::Catalog,
        "Other".into(),
    )
    .ok_or("missing cached sibling")?;
    if executions(&host, "resolve_metadata_object")? != sibling_executions
        || !Arc::ptr_eq(&sibling, &sibling_again)
    {
        return Err("unrelated object resolution re-executed".into());
    }
    let current = labels(&host, &source)?;
    let update_ms = started.elapsed().as_secs_f64() * 1000.0;
    if !current.contains("DraftField") || current.contains("SourceField") {
        return Err("draft completion stale".into());
    }
    if !Arc::ptr_eq(&index, config_index(host.raw_database(), listing)) {
        return Err("content edit rebuilt structure index".into());
    }
    let other_draft = draft(
        &reader,
        &other_path,
        &other_text,
        "OtherField",
        "ChangedOther",
    )?;
    metadata
        .apply_overlay(&other_path, metadata.revision(), 1, &other_draft, &request)
        .map_err(|error| format!("{error:?}"))?;
    update(
        &mut host,
        other_input,
        metadata.owner(&other_path).ok_or("missing input")?,
    )?;
    let before = executions(&host, "infer_method_query")?;
    if before == 0 {
        return Err("BSL inference was not observed".into());
    }
    if labels(&host, &source)? != current || executions(&host, "infer_method_query")? != before {
        return Err("unrelated metadata edit re-inferred BSL".into());
    }
    metadata
        .apply_overlay(
            &sample_path,
            metadata.revision(),
            2,
            "document: [",
            &request,
        )
        .map_err(|error| format!("{error:?}"))?;
    if !matches!(
        metadata.owner(&sample_path).ok_or("missing input")?.state,
        InputState::Unavailable(_)
    ) || metadata.summary().metadata_input_complete
    {
        return Err("invalid YAML accepted as complete".into());
    }
    update(
        &mut host,
        sample_input,
        metadata.owner(&sample_path).ok_or("missing input")?,
    )?;
    if resolve_metadata_object(
        host.raw_database(),
        listing,
        MdoType::Catalog,
        "Sample".into(),
    )
    .is_some()
    {
        return Err("pending canonical value fell back".into());
    }
    if labels(&host, &source)?.contains("DraftField") {
        return Err("invalid draft returned stale completion".into());
    }
    metadata
        .close_overlay(&sample_path, metadata.revision(), &request)
        .map_err(|error| format!("{error:?}"))?;
    update(
        &mut host,
        sample_input,
        metadata.owner(&sample_path).ok_or("missing input")?,
    )?;
    if !labels(&host, &source)?.contains("SourceField") {
        return Err("restored completion missing".into());
    }
    if fs::read_to_string(source.join(sample_path.as_str()))? != sample_text
        || fs::read_to_string(source.join(other_path.as_str()))? != other_text
    {
        return Err("saved YAML changed".into());
    }
    if source.join("Configuration.xml").exists() {
        return Err("XML metadata appeared".into());
    }
    if executions(&host, "parse_mdo_query")? != 0 {
        return Err("canonical metadata used the XML parser".into());
    }
    println!(
        "{}",
        json!({"same_host":true,"draft_completion_applied":true,"sibling_resolution_executions":0,
        "observed_initial_resolutions":sibling_executions,"observed_bsl_inferences":before,"xml_parse_executions":0,"metadata_complete":false,
        "unrelated_bsl_inference_executions":0,"structure_index_retained":true,"pending_has_no_fallback":true,
        "invalid_yaml_rejected":true,"restored_completion_applied":true,"disk_unchanged":true,"update_ms":update_ms,
        "metadata_coverage":"catalog-and-direct-attribute-names","xml_exported":false,
        "shared_metadata_inputs":true,"input_summary":metadata.summary()})
    );
    Ok(())
}
