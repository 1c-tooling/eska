//! Whole-project semantic checking requires an explicitly composed owning configuration.

use eska::project_core::ProjectType;
use std::fs;

use super::{run, support::TestDirectory};
#[path = "../support/external_project.rs"]
mod fixture;

/// EPF/ERF own canonical admission cannot turn absent global/common metadata into false findings.
#[test]
fn external_semantic_checker_requires_configuration_context_with_stable_locale_independent_json() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let directory = TestDirectory::new();
        let root = fixture::project(&directory.0, kind);
        let paths = fixture::paths(&root);
        let physical = root.join("src").join(paths.object_module.as_path());
        let body = "Процедура Проба()\n    НеизвестныйОбщийМодуль.Выполнить();\nКонецПроцедуры\n";
        fs::write(&physical, body).unwrap();
        let preserved = [
            &paths.owner,
            &paths.object_module,
            &paths.form_module,
            &paths.form_owner,
            &paths.dialog,
        ]
        .map(|source| {
            let file = root.join("src").join(source.as_path());
            let bytes = fs::read(&file).unwrap();
            (file, bytes)
        });
        let mut outputs = Vec::new();
        for lang in ["ru-RU", "en-US"] {
            let result = run(&root, lang, &["--profile", "semantic", "--format", "json"]);
            assert_eq!(result.status.code(), Some(3));
            let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(
                value["error"]["code"],
                "analysis_configuration_context_required"
            );
            assert_eq!(
                value["error"]["diagnostics"]["analysis_scope"],
                "canonical_external"
            );
            assert_eq!(
                value["error"]["diagnostics"]["metadata_context_complete"],
                false
            );
            outputs.push(value);
            let human = run(&root, lang, &["--profile", "semantic"]);
            assert_eq!(human.status.code(), Some(3));
            assert!(!human.stderr.is_empty());
        }
        assert_eq!(outputs[0], outputs[1]);
        for (file, bytes) in preserved {
            assert_eq!(fs::read(file).unwrap(), bytes);
        }
        assert_eq!(fs::read_to_string(&physical).unwrap(), body);
        assert!(!root.join(".eska").exists());
    }
}
