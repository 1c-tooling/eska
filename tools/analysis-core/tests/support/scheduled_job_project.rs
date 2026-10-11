//! A real source import supplies current scheduled jobs to SDK and process consumers.

use std::{fs, path::Path};

/// Extend only this test's owned native fixture, then select a separately imported source directory.
pub fn add_jobs(root: &Path, native: &Path) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace("</ChildObjects>", "<ScheduledJob>BeforeRun</ScheduledJob><ScheduledJob>Neighbor</ScheduledJob></ChildObjects>"),
    ).unwrap();
    fs::create_dir(native.join("ScheduledJobs")).unwrap();
    for (name, uuid, method) in [
        ("BeforeRun", "50000000-0000-4000-8000-000000000001", "Saved"),
        ("Neighbor", "60000000-0000-4000-8000-000000000001", "Other"),
    ] {
        fs::write(native.join(format!("ScheduledJobs/{name}.xml")),format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><ScheduledJob uuid="{uuid}"><Properties><Name>{name}</Name><MethodName>CommonModule.Tools.{method}</MethodName><Predefined>false</Predefined><Use>true</Use></Properties></ScheduledJob></MetaDataObject>"#)).unwrap();
    }
    eska::project_core::source_format::import_designer(native, &root.join("src-with-jobs"))
        .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-jobs'",
        ),
    )
    .unwrap();
}
