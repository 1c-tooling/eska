//! Runtime dependency boundaries are checked against Cargo's resolved normal graph.

use std::{collections::BTreeSet, path::Path, process::Command};

const BOUNDARIES: &[(&str, &[&str])] = &[
    (
        "eska",
        &[
            "eska-project",
            "eska-build",
            "eska-build-runtime",
            "eska-vcs",
            "eska-analyzer",
            "eska-check",
            "gix",
        ],
    ),
    (
        "eska-project-core",
        &[
            "eska-presentation",
            "eska-tool-runtime",
            "eska-project",
            "eska-build-runtime",
            "eska-vcs",
            "eska-analyzer",
            "gix",
            "clap",
            "tokio",
            "reqwest",
            "crossterm",
        ],
    ),
    (
        "eska-project",
        &[
            "eska",
            "eska-build",
            "eska-build-runtime",
            "eska-vcs",
            "gix",
        ],
    ),
    (
        "eska-check",
        &[
            "eska",
            "eska-project",
            "eska-build-runtime",
            "eska-vcs",
            "gix",
            "eska-terminal",
        ],
    ),
    (
        "eska-build",
        &["eska", "eska-project", "eska-vcs", "eska-analyzer", "gix"],
    ),
    (
        "eska-vcs",
        &[
            "eska",
            "eska-project",
            "eska-build-runtime",
            "eska-analyzer",
        ],
    ),
];

/// Optional tools must remain independently buildable without another runtime engine.
#[test]
fn normal_component_graphs_preserve_runtime_boundaries() {
    for (package, forbidden) in BOUNDARIES {
        let output = Command::new(env!("CARGO"))
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
            .args([
                "tree",
                "--package",
                package,
                "--edges",
                "normal",
                "--prefix",
                "none",
                "--format",
                "{p}",
                "--all-features",
                "--locked",
                "--offline",
            ])
            .output()
            .expect("resolve the local Cargo graph");
        assert!(
            output.status.success(),
            "{package}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).expect("Cargo package names are UTF-8");
        let names: BTreeSet<_> = text
            .lines()
            .filter_map(|line| line.split_ascii_whitespace().next())
            .collect();
        assert!(names.contains(package), "empty graph for {package}: {text}");
        for dependency in *forbidden {
            assert!(
                !names.contains(dependency),
                "{package} unexpectedly links {dependency}:\n{text}"
            );
        }
    }
}
