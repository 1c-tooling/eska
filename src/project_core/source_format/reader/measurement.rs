//! Opt-in reader measurements on explicit, read-only playground source trees.

use std::{fs, io::Write, path::PathBuf, time::Instant};

use super::SourceReader;

/// Linux reports process memory; another host explicitly reports unavailable measurements.
fn memory() -> serde_json::Value {
    let Ok(text) = fs::read_to_string("/proc/self/status") else {
        return serde_json::Value::Null;
    };
    let value = |key| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|value| value * 1024)
    };
    serde_json::json!({"rss_bytes": value("VmRSS:"), "peak_rss_bytes": value("VmHWM:")})
}

#[test]
#[ignore = "Explicit read-only playground input and a fresh measurement report are required"]
fn real_playground_reader() {
    let playground = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let source = PathBuf::from(std::env::var_os("ESKA_FORMAT_INPUT").unwrap())
        .canonicalize()
        .unwrap();
    let output = PathBuf::from(std::env::var_os("ESKA_FORMAT_READER_REPORT").unwrap());
    assert!(source.starts_with(&playground));
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&playground)
    );
    let mut report = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    let initial = memory();
    let started = Instant::now();
    let reader = SourceReader::open(&source).unwrap();
    let open = serde_json::json!({"seconds":started.elapsed().as_secs_f64(),"memory":memory()});
    println!("open: {open}");
    let mut owners = 0;
    let mut declarations = 0;
    let mut largest_owner_bytes = 0;
    let started = Instant::now();
    for path in reader.owner_paths() {
        let snapshot = reader.read_owner(path).unwrap();
        owners += 1;
        declarations += snapshot.declarations.len();
        largest_owner_bytes =
            largest_owner_bytes.max(fs::metadata(source.join(path.as_path())).unwrap().len());
    }
    let all = serde_json::json!({"seconds":started.elapsed().as_secs_f64(),"memory":memory(),
        "owners":owners,"declarations":declarations,"largest_owner_bytes":largest_owner_bytes});
    println!("all_owners: {all}");
    let result = serde_json::json!({"schema_version":1,"optimized":!cfg!(debug_assertions),
        "initial_memory":initial,"open":open,"all_owners":all,
        "manifest_sha256":reader.configuration().manifest_sha256});
    report
        .write_all(&serde_json::to_vec_pretty(&result).unwrap())
        .unwrap();
}
