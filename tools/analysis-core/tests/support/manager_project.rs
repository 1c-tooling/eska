//! Current manager exports are declared in an owned project, alongside independent neighboring bodies.

use std::{fs, path::Path};

use super::predefined_project;

pub const SAVED: &str = "\u{feff}// Я😀\r\nProcedure SavedMethod(Value) Export\r\nEndProcedure\r\nProcedure HiddenMethod()\r\nEndProcedure\r\n";
pub const CURRENT: &str =
    "\u{feff}// Я😀 current\r\nProcedure CurrentMethod(Value = \"😀\") Export\r\nEndProcedure\r\n";

/// Add declared text/binary manager bodies, then import one fresh source without touching the earlier fixture.
pub fn add_managers(root: &Path, native: &Path) {
    predefined_project::add_predefined(root, native);
    for (path, text) in [
        ("Catalogs/Items/Ext/ManagerModule.bsl", SAVED),
        (
            "Catalogs/Neighbor/Ext/ManagerModule.bsl",
            "Procedure NeighborMethod() Export\nEndProcedure\n",
        ),
    ] {
        fs::write(native.join(path), text).unwrap();
    }
    fs::create_dir_all(native.join("Catalogs/Absent/Ext")).unwrap();
    fs::write(
        native.join("Catalogs/Absent/Ext/ManagerModule.bin"),
        [0xff, 0xfe],
    )
    .unwrap();
    eska::project_core::source_format::import_designer(native, &root.join("src-with-managers"))
        .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace("source='src-with-predefined'", "source='src-with-managers'"),
    )
    .unwrap();
}
