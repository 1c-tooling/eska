//! Designer 2.20 skeletons verified by independent real-platform build and unpack.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use crate::project_core::{
    ProjectType,
    source_format::{Error, SourcePath},
};

use super::{ImportedProject, NewProjectOptions, finish_stage, identity, io_error};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
const XR: &str = "http://v8.1c.ru/8.3/xcf/readable";
const CONFIGURATION_CLASSES: [&str; 7] = [
    "9cd510cd-abfc-11d4-9434-004095e12fc7",
    "9fcd25a0-4822-11d4-9414-008048da11f9",
    "e3687481-0a87-462c-a166-9f34594f9bba",
    "9de14907-ec23-4a07-96f0-85521cb6b53b",
    "51f2d5d8-ea4d-4064-8892-82951750031e",
    "e68182ea-4237-4383-967f-90c1e3370bc7",
    "fb282519-d103-4dd3-bc12-cb271d631dfc",
];
const PROCESSING_CLASSES: [&str; 1] = ["c3831ec8-d8d5-4f93-8a22-f9bfae07327f"];
const REPORT_CLASSES: [&str; 1] = ["e41aff26-25cf-4bb6-b6c1-3f478a75f374"];

pub(super) struct NativeStage {
    pub(super) path: PathBuf,
}

impl NativeStage {
    /// Claim only a fresh sibling beneath the destination's existing canonical parent.
    pub(super) fn claim(destination: &Path) -> Result<Self, Error> {
        let file = destination
            .file_name()
            .and_then(|file| file.to_str())
            .ok_or_else(|| Error::root("source_path_invalid"))?;
        SourcePath::parse(file)?;
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let parent = parent.canonicalize().map_err(|error| io_error(&error))?;
        match fs::symlink_metadata(parent.join(file)) {
            Ok(_) => return Err(Error::root("source_destination_exists")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&error)),
        }
        let path = parent.join(format!(".eska-new-{}", identity::fresh()?.as_str()));
        fs::create_dir(&path).map_err(|error| io_error(&error))?;
        Ok(Self { path })
    }

    /// Preserve the importer result only after removing this operation's own temporary input.
    pub(super) fn finish(
        self,
        result: Result<ImportedProject, Error>,
    ) -> Result<ImportedProject, Error> {
        finish_stage(&self.path, result)
    }
}

/// Names are native identifiers and portable external-root filename stems; no shell/XML escaping is guessed.
pub(super) fn validate_name(name: &str) -> Result<(), Error> {
    let mut chars = name.chars();
    if !chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
        || !chars.all(|ch| ch == '_' || ch.is_alphanumeric())
        || SourcePath::parse(&format!("{name}.xml")).is_err()
    {
        return Err(Error::root("source_metadata_name_invalid").in_field("metadata_name"));
    }
    Ok(())
}

/// Empty composition keeps platform defaults; only identities and the selected initial version are supplied.
pub(super) fn write(root: &Path, options: &NewProjectOptions, name: &str) -> Result<(), Error> {
    let kind = options.project_type;
    let (tag, file) = match kind {
        ProjectType::Configuration | ProjectType::Extension => {
            ("Configuration", "Configuration.xml".to_owned())
        }
        ProjectType::Processing => ("ExternalDataProcessor", format!("{name}.xml")),
        ProjectType::Report => ("ExternalReport", format!("{name}.xml")),
    };
    let info = internal_info(kind, name)?;
    let properties = match kind {
        ProjectType::Configuration => {
            format!("<Name>{name}</Name><Version>{}</Version>", options.version)
        }
        // ObjectBelonging distinguishes a real CFE: purpose alone serialized as an ordinary CF.
        ProjectType::Extension => format!(
            "<ObjectBelonging>Adopted</ObjectBelonging><Name>{name}</Name><Version>{}</Version><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>",
            options.version
        ),
        ProjectType::Processing | ProjectType::Report => format!("<Name>{name}</Name>"),
    };
    let xml = format!(
        "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<MetaDataObject xmlns=\"{MD}\" xmlns:xr=\"{XR}\" version=\"2.20\">\r\n<{tag} uuid=\"{}\"><InternalInfo>{info}</InternalInfo><Properties>{properties}</Properties><ChildObjects/></{tag}>\r\n</MetaDataObject>\r\n",
        identity::fresh()?.as_str()
    );
    fs::write(root.join(file), xml).map_err(|error| io_error(&error))?;
    if matches!(kind, ProjectType::Processing | ProjectType::Report) {
        let module = root.join(name).join("Ext/ObjectModule.bsl");
        fs::create_dir_all(
            module
                .parent()
                .ok_or_else(|| Error::root("source_path_invalid"))?,
        )
        .map_err(|error| io_error(&error))?;
        fs::write(module, object_module(options)).map_err(|error| io_error(&error))?;
    }
    Ok(())
}

/// Platform class constants remain fixed; all object/type/value identities belong only to this new project.
fn internal_info(kind: ProjectType, name: &str) -> Result<String, Error> {
    let mut info = String::new();
    let classes = match kind {
        ProjectType::Configuration | ProjectType::Extension => CONFIGURATION_CLASSES.as_slice(),
        ProjectType::Processing => PROCESSING_CLASSES.as_slice(),
        ProjectType::Report => REPORT_CLASSES.as_slice(),
    };
    for class in classes {
        write!(info, "<xr:ContainedObject><xr:ClassId>{class}</xr:ClassId><xr:ObjectId>{}</xr:ObjectId></xr:ContainedObject>", identity::fresh()?.as_str())
            .map_err(|_| Error::root("source_schema_invalid"))?;
    }
    let generated = match kind {
        ProjectType::Processing => Some("ExternalDataProcessorObject"),
        ProjectType::Report => Some("ExternalReportObject"),
        ProjectType::Configuration | ProjectType::Extension => None,
    };
    if let Some(generated) = generated {
        write!(info, "<xr:GeneratedType name=\"{generated}.{name}\" category=\"Object\"><xr:TypeId>{}</xr:TypeId><xr:ValueId>{}</xr:ValueId></xr:GeneratedType>", identity::fresh()?.as_str(), identity::fresh()?.as_str())
            .map_err(|_| Error::root("source_schema_invalid"))?;
    }
    Ok(info)
}

/// EPF/ERF store their sole version in a documented exported object-module function, never in metadata.
fn object_module(options: &NewProjectOptions) -> String {
    format!(
        "\u{feff}#Область ПрограммныйИнтерфейс\r\n\r\n// Возвращает версию внешнего объекта.\r\n//\r\n// Возвращаемое значение:\r\n//  Строка - Версия внешнего объекта.\r\nФункция Версия() Экспорт\r\n\tВозврат \"{}\";\r\nКонецФункции\r\n\r\n#КонецОбласти\r\n",
        options.version
    )
}
