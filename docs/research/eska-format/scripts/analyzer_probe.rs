//! A bounded, immutable-snapshot experiment; this is not the installed analyzer.

use std::{collections::BTreeSet, error::Error, fs, path::Path, sync::Arc, time::Instant};

use base_db::{Locale, SourceDatabase, SourceRootId};
use bsl_metadata::{Attribute, AttributeType, Configuration, MdoType, MetadataObject};
use eska::project_core::{
    MetadataKind,
    source_format::{SourcePath, SourceReader, import_designer},
};
use ide_db::GraphConfigCache;
use ide_host_core::{AnalysisHost, FileTextSource, build_source_root, set_file_text_source};
use serde_json::{Value, json};
use vfs::FileId;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Own one independent analysis context per metadata revision; never mutate its cache.
fn host(root: &Path, configuration: Configuration, code: &str) -> AnalysisHost {
    let mut host = AnalysisHost::default();
    let cache = Arc::new(GraphConfigCache::new());
    cache.insert(root.to_path_buf(), Arc::new(configuration));
    let database = host.raw_database_mut();
    database.set_graph_config_cache(cache);
    database.set_all_config_paths(vec![(None, root.to_path_buf())]);
    database.set_workspace_load_complete(true);
    let files = [(FileId(0), root.join("CommonModules/Probe/Ext/Module.bsl"))];
    database.set_source_root(SourceRootId(0), build_source_root(&files));
    database.set_file_source_root(FileId(0), SourceRootId(0));
    set_file_text_source(database, FileId(0), FileTextSource::Overlay(code));
    host
}

/// Project catalog names and direct custom attribute names; type parity is explicitly absent.
fn project_configuration(
    reader: &SourceReader,
    overlay: Option<(&SourcePath, &str)>,
) -> Result<(Configuration, Value)> {
    let root = &reader.configuration().source.document.descriptor.object;
    let name = reader
        .configuration()
        .declarations
        .first()
        .ok_or("missing configuration")?
        .name
        .clone();
    // Deserialization sets the native root identity; add_metadata_object builds lookup indexes.
    let mut config: Configuration =
        serde_json::from_value(json!({"uuid":root.uuid.as_str(),"name":name}))?;
    let mut catalogs = 0;
    let mut attributes = 0;
    let mut skipped = 0;
    for path in reader.owner_paths() {
        if path.as_str() == "configuration.yaml" {
            continue;
        }
        let snapshot = match overlay {
            Some((target, text)) if target == path => reader.parse_owner(path, text)?,
            _ => reader.read_owner(path)?,
        };
        let main = &snapshot.source.document.descriptor.object;
        if main.kind != MetadataKind::Catalog {
            skipped += 1;
            continue;
        }
        let declaration = snapshot
            .declarations
            .iter()
            .find(|item| item.uuid == main.uuid)
            .ok_or("missing catalog")?;
        let mut object = MetadataObject::new(MdoType::Catalog, &declaration.name);
        object.uuid = Some(main.uuid.as_str().parse()?);
        for child in &snapshot.declarations {
            if child.kind == MetadataKind::Attribute && child.parent.as_ref() == Some(&main.uuid) {
                object.attributes.push(Attribute {
                    name: child.name.clone(),
                    name_en: None,
                    attr_type: AttributeType::Unknown,
                });
                attributes += 1;
            }
        }
        config.add_metadata_object(object);
        catalogs += 1;
    }
    Ok((
        config,
        json!({"catalogs":catalogs,"attributes_with_unknown_type":attributes,"unsupported_owners":skipped,"coverage":"catalog-and-direct-attribute-names"}),
    ))
}

/// Query the real inference/completion pipeline at an identifier immediately after a dot.
fn completions(host: &AnalysisHost, root: &Path, code: &str) -> Result<BTreeSet<String>> {
    let offset = code.find("Item.x").ok_or("missing cursor")? + "Item.".len();
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

/// Record process memory, not compiler memory; unsupported operating systems return null.
fn memory() -> Value {
    let Ok(status) = fs::read_to_string("/proc/self/status") else {
        return Value::Null;
    };
    let value = |key: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|text| text.split_whitespace().next())
            .and_then(|text| text.parse::<u64>().ok())
    };
    json!({"rss_kib":value("VmRSS:"),"high_water_kib":value("VmHWM:"),"swap_kib":value("VmSwap:")})
}

/// Verify metadata and BSL overlays with retained, unchanged older snapshots.
fn demo(native: &Path, output: &Path) -> Result<Value> {
    import_designer(native, output)?;
    let root = fs::canonicalize(output)?;
    if root.join("Configuration.xml").exists() {
        return Err("unexpected XML metadata root".into());
    }
    let reader = SourceReader::open(&root)?;
    let path = SourcePath::parse("Catalogs/Sample/object.yaml")?;
    let saved = reader.read_owner(&path)?;
    let text = fs::read_to_string(root.join(path.as_str()))?;
    let field = saved
        .declarations
        .iter()
        .find(|item| item.kind == MetadataKind::Attribute && item.name == "SourceField")
        .ok_or("missing probe field")?;
    let mut draft = text.clone();
    draft.replace_range(field.name_range.clone(), "DraftField");
    let parsed = reader.parse_owner(&path, &draft)?;
    if parsed.source_sha256 == saved.source_sha256 {
        return Err("revision did not change".into());
    }
    let code = "// Demonstrates metadata-driven completion.\nПроцедура Probe()\n    Item = Справочники.Sample.СоздатьЭлемент();\n    Item.x = 0;\nКонецПроцедуры\n";
    let started = Instant::now();
    let (configuration, coverage) = project_configuration(&reader, None)?;
    let old_host = host(&root, configuration, code);
    if !old_host.analysis().parse(FileId(0)).errors().is_empty() {
        return Err("probe BSL has syntax errors".into());
    }
    let old = completions(&old_host, &root, code)?;
    let first_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let (configuration, _) = project_configuration(&reader, Some((&path, &draft)))?;
    let draft_host = host(&root, configuration, code);
    let new = completions(&draft_host, &root, code)?;
    let draft_ms = started.elapsed().as_secs_f64() * 1000.0;
    if !old.contains("SourceField")
        || old.contains("DraftField")
        || !new.contains("DraftField")
        || new.contains("SourceField")
    {
        return Err(format!("wrong metadata completion: old={old:?}; draft={new:?}").into());
    }
    if completions(&old_host, &root, code)? != old {
        return Err("old snapshot changed".into());
    }
    if reader.parse_owner(&path, "document: [").is_ok() {
        return Err("invalid YAML accepted".into());
    }
    let changed_code = code.replace("Справочники.Sample.СоздатьЭлемент()", "Новый Структура");
    let (configuration, _) = project_configuration(&reader, Some((&path, &draft)))?;
    let bsl_host = host(&root, configuration, &changed_code);
    if completions(&bsl_host, &root, &changed_code)?.contains("DraftField") {
        return Err("BSL overlay ignored".into());
    }
    if fs::read_to_string(root.join(path.as_str()))? != text {
        return Err("saved YAML changed".into());
    }
    let bsl_revision = base_db::content_revision(code);
    let draft_bsl_revision = base_db::content_revision(&changed_code);
    if bsl_revision == draft_bsl_revision {
        return Err("BSL revision did not change".into());
    }
    Ok(
        json!({"saved_revision":saved.source_sha256,"draft_revision":parsed.source_sha256,"manifest_revision":parsed.manifest_sha256,
        "bsl_revision":bsl_revision,"draft_bsl_revision":draft_bsl_revision,"bsl_syntax_errors":0,
        "saved_field_visible":true,"draft_field_visible":true,"old_snapshot_retained":true,"bsl_overlay_applied":true,
        "invalid_yaml_rejected":true,"disk_unchanged":true,"xml_exported":false,"first_ms":first_ms,"draft_ms":draft_ms,
        "memory":memory(),"coverage":coverage}),
    )
}

/// Measure one real saved source with the same deliberately bounded metadata projection.
fn measure(root: &Path) -> Result<Value> {
    let root = fs::canonicalize(root)?;
    let started = Instant::now();
    let reader = SourceReader::open(&root)?;
    let open_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let (configuration, coverage) = project_configuration(&reader, None)?;
    let project_ms = started.elapsed().as_secs_f64() * 1000.0;
    let name = configuration
        .metadata_objects()
        .first()
        .ok_or("no catalogs")?
        .name
        .clone();
    let code = format!(
        "// Completion probe.\nПроцедура Probe()\n    Item = Справочники.{name}.СоздатьЭлемент();\n    Item.x = 0;\nКонецПроцедуры\n"
    );
    let started = Instant::now();
    let host = host(&root, configuration, &code);
    let labels = completions(&host, &root, &code)?;
    Ok(
        json!({"open_ms":open_ms,"project_ms":project_ms,"completion_ms":started.elapsed().as_secs_f64()*1000.0,
        "completion_count":labels.len(),"memory":memory(),"coverage":coverage,"xml_exported":false}),
    )
}

/// Dispatch explicit owned paths; emit evidence only after every invariant succeeds.
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match args.as_slice() {
        [mode, native, output] if mode == "demo" => demo(Path::new(native), Path::new(output))?,
        [mode, root] if mode == "measure" => measure(Path::new(root))?,
        _ => return Err("expected demo NATIVE NEW_SOURCE or measure SOURCE".into()),
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
