//! A guarded playground project keeps each direct call's source identity and independent input reserve together.

use std::path::PathBuf;

use super::*;
use crate::{common_module_project, manager_project};

pub(super) struct MemberCall {
    _directory: support::TestDirectory,
    pub root: PathBuf,
    folder: &'static str,
    target: &'static str,
    saved: &'static str,
    method: &'static str,
    pub first_revision: &'static str,
    pub body: String,
    at: usize,
    pub current: String,
}

impl MemberCall {
    /// Import a declared target into one unique fixture and place a non-BMP character before the argument cursor.
    pub fn new(manager: bool) -> Self {
        let (folder, target, saved, current, receiver, method, first_revision) = if manager {
            (
                "src-with-managers",
                "Catalogs/Items/Ext/ManagerModule.bsl",
                manager_project::SAVED,
                manager_project::CURRENT.replace("CurrentMethod", "SavedMethod"),
                "Catalogs.Items",
                "SavedMethod",
                "5",
            )
        } else {
            (
                "src-with-common-modules",
                "CommonModules/Utility/Ext/Module.bsl",
                common_module_project::SAVED,
                common_module_project::CURRENT.replace("CurrentUtility", "SavedUtility"),
                "Utility",
                "SavedUtility",
                "4",
            )
        };
        let prefix =
            format!("\u{feff}// Я😀\r\nProcedure Run()\r\n Я=\"😀\"; {receiver}.{method}(");
        let body = format!("{prefix});\r\nEndProcedure\r\n");
        let (directory, root) = support::project(&body, false);
        if manager {
            manager_project::add_managers(&root, &directory.0.join("native"));
        } else {
            common_module_project::add_common_modules(&root, &directory.0.join("native"));
        }
        Self {
            _directory: directory,
            root,
            folder,
            target,
            saved,
            method,
            first_revision,
            body,
            at: prefix.len(),
            current: current.replace("Value = \"😀\"", "First = \"😀\", Next = 2"),
        }
    }

    /// Convert the same original argument position after each acknowledged source publication.
    pub fn pull(&self, revision: &str) -> Value {
        pull(
            &self.root,
            MODULE,
            self.folder,
            revision,
            position(&self.body, self.at),
        )
    }

    /// Editor events address only the declared target within this guarded project.
    pub fn target_uri(&self) -> String {
        url::Url::from_file_path(self.root.join(self.folder).join(self.target))
            .unwrap()
            .to_string()
    }

    /// Parameter ranges and caller authority are independent of target-body changes.
    pub fn verify(&self, result: &Value, revision: &str) {
        verify(result, &self.body, MODULE, revision, self.at, true);
    }

    /// A cold request reads exactly its caller context and selected owner/resource/body.
    pub fn verify_first(&self, first: &Value) {
        self.verify(&first["result"], self.first_revision);
        assert!(
            first["result"]["signatures"][0]["label"]
                .as_str()
                .unwrap()
                .contains(self.method)
        );
        assert_eq!(
            first["result"]["signatures"][0]["parameters"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            first["result"]["eska"]["resources"]["module_text_bytes"],
            self.body.len() + self.saved.len()
        );
        let mut paths = vec![MODULE, "CommonModules/Tools/object.yaml", self.target];
        if self.folder == "src-with-managers" {
            paths.extend([
                "Catalogs/Items/object.yaml",
                "Catalogs/Items/Ext/Predefined.xml",
            ]);
        } else {
            paths.push("CommonModules/Utility/object.yaml");
        }
        let bytes = paths
            .into_iter()
            .map(|p| {
                fs::metadata(self.root.join(self.folder).join(p))
                    .unwrap()
                    .len()
            })
            .sum::<u64>();
        assert_eq!(
            first["result"]["eska"]["resources"]["total_source_bytes"],
            bytes
        );
    }

    /// All current-input notifications leave the original saved caller and callee unchanged.
    pub fn verify_saved(&self) {
        let source = self.root.join(self.folder);
        assert_eq!(
            fs::read_to_string(source.join(self.target)).unwrap(),
            self.saved
        );
        assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), self.body);
    }
}
