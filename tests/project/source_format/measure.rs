//! Explicit release-mode measurements of core calls without acceptance-copy overhead.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use eska::project::source_format::{
    AcceptedBase, AcceptedRef, accept_source, export_designer, export_protected, import_designer,
};

/// Inputs are read-only; every measurement claims its own new playground output.
struct Inputs {
    source: PathBuf,
    next_source: Option<PathBuf>,
    output: PathBuf,
    base: Option<(PathBuf, AcceptedRef)>,
}

/// Resolve explicit measurement paths without hardcoded corpus names or repository writes.
fn inputs() -> Inputs {
    let root = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let source = PathBuf::from(std::env::var_os("ESKA_FORMAT_INPUT").unwrap())
        .canonicalize()
        .unwrap();
    let output = PathBuf::from(std::env::var_os("ESKA_FORMAT_OUTPUT").unwrap());
    assert!(source.starts_with(&root));
    let next_source = std::env::var_os("ESKA_FORMAT_NEXT_INPUT").map(|path| {
        let path = PathBuf::from(path).canonicalize().unwrap();
        assert!(path.starts_with(&root));
        path
    });
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&root)
    );
    assert!(!output.exists());
    fs::create_dir(&output).unwrap();
    let base = std::env::var_os("ESKA_FORMAT_BASE").map(|path| {
        let path = PathBuf::from(path).canonicalize().unwrap();
        assert!(path.starts_with(&root));
        (
            path,
            AcceptedRef {
                sha256: std::env::var("ESKA_FORMAT_REFERENCE").unwrap(),
            },
        )
    });
    Inputs {
        source,
        next_source,
        output,
        base,
    }
}

/// Linux RSS is observed from this process; other hosts report unavailable data explicitly.
fn memory() -> serde_json::Value {
    let Ok(text) = fs::read_to_string("/proc/self/status") else {
        return serde_json::Value::Null;
    };
    let read = |key| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|value| value * 1024)
    };
    serde_json::json!({"rss_bytes":read("VmRSS:"),"peak_rss_bytes":read("VmHWM:")})
}

/// Record only core execution, keeping filesystem byte accounting outside the timed span.
fn run<T>(label: &str, records: &mut Vec<serde_json::Value>, operation: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = operation();
    let seconds = started.elapsed().as_secs_f64();
    let record = serde_json::json!({"operation":label,"seconds":seconds,"memory":memory()});
    println!("{record}");
    records.push(record);
    value
}

/// Opening a base is measured separately from calls which reuse the already verified model.
fn open(inputs: &Inputs, records: &mut Vec<serde_json::Value>) -> AcceptedBase {
    let (path, reference) = inputs
        .base
        .as_ref()
        .expect("explicit independently pinned base");
    run("baseline_open", records, || {
        AcceptedBase::open(path, reference).unwrap()
    })
}

/// Repeat one export to distinguish first publication from exact-output verification.
fn exports(inputs: &Inputs, records: &mut Vec<serde_json::Value>, protected: bool) {
    let base = protected.then(|| open(inputs, records));
    for index in 0..3 {
        let source = if index == 0 {
            &inputs.source
        } else {
            inputs.next_source.as_ref().unwrap_or(&inputs.source)
        };
        run(&format!("export_{index}"), records, || {
            if let Some(base) = &base {
                let result =
                    export_protected(source, base, &inputs.output.join("generations"), true)
                        .unwrap();
                assert!(result.diagnostics.is_empty());
            } else {
                export_designer(source, &inputs.output.join("xml")).unwrap();
            }
        });
    }
}

/// A result records whether compiler optimization was enabled, not a claim of cold OS caches.
fn persist(
    root: &Path,
    operation: &str,
    records: &[serde_json::Value],
    initial: &serde_json::Value,
) {
    let result = serde_json::json!({"schema_version":1,"operation":operation,
        "optimized":!cfg!(debug_assertions),"os":std::env::consts::OS,
        "initial_memory":initial,"final_memory":memory(),"records":records});
    fs::write(
        root.join("measurement.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "Explicit core measurement; requires read-only playground input, fresh output and selected operation"]
fn real_playground_measure() {
    let initial = memory();
    let inputs = inputs();
    let operation = std::env::var("ESKA_FORMAT_OPERATION").unwrap();
    assert!(inputs.next_source.is_none() || operation == "protected");
    let mut records = Vec::new();
    match operation.as_str() {
        "import" => {
            run("import", &mut records, || {
                import_designer(&inputs.source, &inputs.output.join("source")).unwrap()
            });
        }
        "export" => exports(&inputs, &mut records, false),
        "protected" => exports(&inputs, &mut records, true),
        "baseline" => {
            let reference = run("baseline_accept", &mut records, || {
                accept_source(&inputs.source, &inputs.output.join("base"), &[]).unwrap()
            });
            let base = run("baseline_open", &mut records, || {
                AcceptedBase::open(&inputs.output.join("base"), &reference).unwrap()
            });
            run("baseline_restore", &mut records, || {
                base.restore(&inputs.output.join("restored")).unwrap();
            });
        }
        _ => panic!("unsupported measurement operation"),
    }
    persist(&inputs.output, &operation, &records, &initial);
}
