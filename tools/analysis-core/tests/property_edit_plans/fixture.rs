//! Small accepted metadata fixtures belong exclusively to the property-plan process tests.

use std::{fs, path::PathBuf};

use eska::project_core::source_format::{AcceptedRef, NativeUuid, accept_source, import_designer};

use super::support;

pub const OWNER: &str = "CommonModules/Tools/object.yaml";
pub const UUID: &str = "20000000-0000-4000-8000-000000000001";

/// A separately pinned base shares the same source fixture, with an optional accepted freeze.
pub fn project(frozen: bool) -> (support::TestDirectory, PathBuf, AcceptedRef) {
    let (directory, root) = support::project("// Saved module.\n", false);
    let native = directory.0.join("native");
    let path = native.join("CommonModules/Tools.xml");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap().replace(
            "<Server>true</Server>",
            "<Server>true</Server><Comment>Saved comment</Comment>",
        ),
    )
    .unwrap();
    import_designer(&native, &root.join("src-plans")).unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace(
            "source_format='eska'",
            "source='src-plans'\nsource_format='eska'",
        ),
    )
    .unwrap();
    fs::create_dir(root.join(".eska")).unwrap();
    let freeze = if frozen {
        vec![NativeUuid::parse(UUID).unwrap()]
    } else {
        Vec::new()
    };
    let reference = accept_source(
        &root.join("src-plans"),
        &root.join(".eska/accepted"),
        &freeze,
    )
    .unwrap();
    (directory, root, reference)
}
