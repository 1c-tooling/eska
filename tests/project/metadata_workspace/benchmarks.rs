//! Reproducible read-only backend measurements; never invoke the 1C platform.
use super::{fixture, object};
use crate::support::TestDir;
use eska::project::{
    configurator::TreeOptions,
    metadata_model::{MetadataKind, NodeId, ProjectScope},
    metadata_workspace::{
        MetadataWorkspace,
        search::{IndexState, SearchOptions},
    },
};
use std::{fmt::Write, fs, path::Path, time::Instant};

/// Measure one fresh session; repeated runs use the OS page cache, not a rebooted machine.
fn sample(root: &Path, cached: bool) -> serde_json::Value {
    let start = Instant::now();
    let mut workspace = if cached {
        MetadataWorkspace::open_cached(root, &[], false)
    } else {
        MetadataWorkspace::open(root, &[], false)
    }
    .unwrap();
    let open_ms = start.elapsed().as_secs_f64() * 1000.0;
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let root_parses = session.cache_stats().parses + session.source_io_stats().root_parses;
    let id = object(MetadataKind::Catalog, "Контрагенты", None);
    let node = NodeId::Object(id.clone());
    let start = Instant::now();
    session.children(&node, TreeOptions::default()).unwrap();
    let expand_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    session.properties(&id).unwrap();
    let properties_ms = start.elapsed().as_secs_f64() * 1000.0;
    let before = session.source_io_stats();
    let start = Instant::now();
    for _ in 0..100 {
        std::hint::black_box(session.node(&node).unwrap());
        session.properties(&id).unwrap();
    }
    let warm_properties_ms = start.elapsed().as_secs_f64() * 10.0;
    assert_eq!(before.xml_reads, session.source_io_stats().xml_reads);
    let start = Instant::now();
    session.source(&node).unwrap();
    let source_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    session.start_search_index();
    let mut max_step_ms = 0.0_f64;
    loop {
        let step = Instant::now();
        let progress = session.index_search_step(8);
        max_step_ms = max_step_ms.max(step.elapsed().as_secs_f64() * 1000.0);
        if progress.state != IndexState::Building {
            break;
        }
    }
    let index_ms = start.elapsed().as_secs_f64() * 1000.0;
    let parses_after_index = session.cache_stats().parses;
    let before = session.source_io_stats();
    let mut search = Vec::new();
    for query in ["Контрагент", "код", "а", "нетТакогоОбъекта"] {
        for _ in 0..10 {
            let start = Instant::now();
            std::hint::black_box(session.search(query, &SearchOptions::default()));
            search.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    search.sort_by(f64::total_cmp);
    assert_eq!(before.xml_reads, session.source_io_stats().xml_reads);
    let root_node = session.root().clone();
    let start = Instant::now();
    for _ in 0..100 {
        session
            .children(
                &root_node,
                TreeOptions {
                    hide_empty_root_sections: true,
                },
            )
            .unwrap();
    }
    let filter_ms = start.elapsed().as_secs_f64() * 10.0;
    let start = Instant::now();
    session
        .changed_paths(&["Catalogs/Контрагенты.xml".into()])
        .unwrap();
    while session.index_search_step(8).state == IndexState::Building {}
    let refresh_ms = start.elapsed().as_secs_f64() * 1000.0;
    let io = session.source_io_stats();
    let cache = session.cache_stats();
    let disk = session.disk_cache_stats();
    serde_json::json!({"open_ms": open_ms, "expand_ms": expand_ms, "properties_ms": properties_ms,
        "warm_properties_ms":warm_properties_ms,"source_ms":source_ms,"index_ms":index_ms,
        "max_step_ms":max_step_ms,"search_median_ms":search[20],"search_p95_ms":search[37],
        "filter_ms":filter_ms,"refresh_ms":refresh_ms,"root_parses":root_parses,
        "parses_after_index":parses_after_index,"refresh_parses":cache.parses-parses_after_index,
        "objects":session.search_progress().indexed_objects,"failures":session.search_progress().failed_descriptors,
        "xml_read_nanos":io.xml_read_nanos,"hash_nanos":cache.hash_nanos + io.root_hash_nanos,
        "xml_reads":io.xml_reads,"xml_bytes":io.xml_bytes,"path_checks":io.path_checks,"metadata_checks":io.metadata_checks,"directory_reads":io.directory_reads,
        "cache_source_bytes":cache.source_bytes,"disk_hits":disk.map(|v| v.hits),"disk_writes":disk.map(|v| v.writes),
        "disk_bytes_read":disk.map(|v|v.bytes_read),"disk_bytes_written":disk.map(|v|v.bytes_written)})
}

/// Generate a declared, broad synthetic configuration without building it or reading BSL.
fn synthetic(root: &Path) {
    fixture(root, "configuration");
    let mut children = String::new();
    for number in 0..2000 {
        let name = if number == 0 {
            "Контрагенты".into()
        } else {
            format!("Справочник{number}")
        };
        write!(children, "<Catalog>{name}</Catalog>").unwrap();
        let mut attributes = String::new();
        for attribute in 0..20 {
            write!(attributes, "<Attribute uuid='id'><Properties><Name>Реквизит{attribute}</Name></Properties></Attribute>").unwrap();
        }
        fs::write(root.join(format!("src/Catalogs/{name}.xml")), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Catalog uuid='id'><Properties><Name>{name}</Name></Properties><ChildObjects>{attributes}</ChildObjects></Catalog></MetaDataObject>")).unwrap();
    }
    fs::write(root.join("src/Configuration.xml"), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='id'><Properties><Name>Синтетическая</Name></Properties><ChildObjects>{children}</ChildObjects></Configuration></MetaDataObject>")).unwrap();
}

/// Run explicitly with `ESKA_METADATA_BENCH=small|synthetic|big` and optional `ESKA_METADATA_CACHE=1`.
#[test]
#[ignore = "explicit backend performance measurement; reads the large fixture only when requested"]
fn measures_metadata_workspace() {
    let owned = TestDir::new();
    let dataset = std::env::var("ESKA_METADATA_BENCH").unwrap_or_else(|_| "small".into());
    let root = match dataset.as_str() {
        "small" => {
            fixture(&owned.0, "configuration");
            owned.0.clone()
        }
        "synthetic" => {
            synthetic(&owned.0);
            owned.0.clone()
        }
        "big" => Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("eska-playground/big_configuration_temp"),
        _ => panic!("unknown benchmark dataset"),
    };
    let cached = std::env::var_os("ESKA_METADATA_CACHE").is_some();
    let runs = std::env::var("ESKA_METADATA_RUNS").map_or(5, |value| value.parse().unwrap());
    for run in 0..runs {
        println!(
            "METADATA_BENCH {}",
            serde_json::json!({"dataset":dataset,"cached":cached,"run":run,"sample":sample(&root,cached)})
        );
    }
}
