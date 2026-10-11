//! An independent callee fixture retains exact original token coordinates and input accounting.

use std::path::PathBuf;

use super::*;
use crate::{common_module_project, manager_project};

pub(super) struct MemberHover {
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

impl MemberHover {
    /// Import one declared target and retain non-BMP text before the original hovered callee.
    pub fn new(manager: bool) -> Self {
        let (folder, target, saved, receiver, method, first_revision) = if manager {
            (
                "src-with-managers",
                "Catalogs/Items/Ext/ManagerModule.bsl",
                manager_project::SAVED,
                "Catalogs.Items",
                "SavedMethod",
                "5",
            )
        } else {
            (
                "src-with-common-modules",
                "CommonModules/Utility/Ext/Module.bsl",
                common_module_project::SAVED,
                "Utility",
                "SavedUtility",
                "4",
            )
        };
        let prefix = format!("\u{feff}// Я😀\r\nProcedure Run()\r\n Я=\"😀\"; {receiver}.");
        let body = format!("{prefix}{method}();\r\nEndProcedure\r\n");
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
            current: format!(
                "// Current purpose Я😀\r\nFunction {method}(Value = \"😀\") Export\r\n Return \"😀\";\r\nEndFunction\r\n"
            ),
        }
    }

    /// Either boundary addresses the same callee while its original byte position stays independent.
    pub fn pull(&self, revision: &str, end: bool) -> Value {
        pull(
            &self.root,
            MODULE,
            self.folder,
            revision,
            position(
                &self.body,
                self.at + if end { self.method.len() } else { 0 },
            ),
        )
    }

    /// Only the declared target is addressed by current-input events.
    pub fn target_uri(&self) -> String {
        url::Url::from_file_path(self.root.join(self.folder).join(self.target))
            .unwrap()
            .to_string()
    }

    /// The hovered range and source hash always belong to the caller's original method token.
    pub fn verify(&self, result: &Value, revision: &str) {
        verify(result, &self.body, MODULE, revision, self.at, self.method);
    }

    /// Initial preparation reads two BSL bodies and exactly their own canonical context.
    pub fn verify_first(&self, first: &Value) {
        self.verify(&first["result"], self.first_revision);
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

    /// Editor changes never replace the saved fixture bytes on disk.
    pub fn verify_saved(&self) {
        let source = self.root.join(self.folder);
        assert_eq!(
            fs::read_to_string(source.join(self.target)).unwrap(),
            self.saved
        );
        assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), self.body);
    }
}
