use std::{fs, process::Command};

use eska::project::{ProjectType, discovery::discover, templates::Template};

use crate::support::TestDir as Fixture;

#[test]
fn all_built_ins_materialize_into_projects_accepted_by_discovery_and_cli() {
    let fixture = Fixture::new();
    for (index, project_type) in [
        ProjectType::Configuration,
        ProjectType::Extension,
        ProjectType::Processing,
        ProjectType::Report,
    ]
    .into_iter()
    .enumerate()
    {
        let root = fixture.0.join(format!("project-{index} каталог"));
        fs::create_dir(&root).expect("create project directory");
        let template = Template::built_in(project_type).expect("render built-in");

        // Only the test writes the plan. Production creation, collision handling
        // and rollback belong to the next milestone's creation workflow.
        for directory in template.directories() {
            fs::create_dir(root.join(directory)).expect("create source directory");
        }
        for file in template.files() {
            fs::write(root.join(file.path()), file.contents()).expect("write template file");
        }

        assert_eq!(fs::read_dir(&root).expect("read project root").count(), 4);
        assert_eq!(
            fs::read(root.join(".gitattributes")).expect("read attributes"),
            include_bytes!("../../assets/project/.gitattributes")
        );
        assert_eq!(
            fs::read(root.join(".gitignore")).expect("read ignore rules"),
            include_bytes!("../../assets/project/.gitignore")
        );
        assert_eq!(
            fs::read_dir(root.join("src"))
                .expect("read sources")
                .count(),
            1
        );
        assert_eq!(
            fs::read(root.join("src/.gitkeep")).expect("read placeholder"),
            b""
        );
        for start in [&root, &root.join("src")] {
            let project = discover(start).expect("discover generated project");
            assert_eq!(project.root(), root);
            assert_eq!(project.source(), root.join("src"));
            assert_eq!(project.configuration().project_type(), project_type);

            for locale in ["ru", "en"] {
                let output = Command::new(env!("CARGO_BIN_EXE_eska"))
                    .current_dir(start)
                    .env_remove("ESKA_LANG")
                    .args(["--lang", locale])
                    .output()
                    .expect("validate generated project with CLI");
                assert_eq!(output.status.code(), Some(0), "{output:?}");
                assert!(output.stdout.is_empty(), "{output:?}");
                assert!(output.stderr.is_empty(), "{output:?}");
            }
        }
    }
}
