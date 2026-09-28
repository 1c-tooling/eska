//! Bounded inspection of the root descriptor, independent of filenames and aliases.

use std::{fs, io::Read, path::Path};

use super::{ArtifactError, io_error};
use crate::project::{ProjectType, designer_xml};

const NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Native metadata identity; workspace aliases and directory names are unrelated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Identity {
    pub project_type: ProjectType,
    pub uuid: String,
    pub name: String,
}

/// Inspect immediate XML descriptors without traversing the configuration tree.
///
/// # Errors
/// Rejects ambiguous, malformed, oversized and linked root descriptors.
pub fn inspect_identity(directory: &Path) -> Result<Option<Identity>, ArtifactError> {
    let mut identity = None;
    for entry in fs::read_dir(directory).map_err(|error| io_error(directory, error))? {
        let entry = entry.map_err(|error| io_error(directory, error))?;
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
            || path
                .file_name()
                .is_some_and(|name| name == "ConfigDumpInfo.xml")
        {
            continue;
        }
        let kind = entry.file_type().map_err(|error| io_error(&path, error))?;
        if kind.is_dir() {
            continue;
        }
        if !kind.is_file() {
            return Err(ArtifactError::UnsafePath(path));
        }
        if let Some(found) = read_identity(&path)? {
            if identity.is_some() {
                return Err(ArtifactError::AmbiguousDescriptor(directory.to_owned()));
            }
            identity = Some(found);
        }
    }
    Ok(identity)
}

/// Parse a single bounded XML document and retain only its root identity.
fn read_identity(path: &Path) -> Result<Option<Identity>, ArtifactError> {
    let mut input = String::new();
    fs::File::open(path)
        .map_err(|error| io_error(path, error))?
        .take(MAX_BYTES + 1)
        .read_to_string(&mut input)
        .map_err(|error| io_error(path, error))?;
    if input.len() as u64 > MAX_BYTES {
        return Err(ArtifactError::InvalidDescriptor(path.to_owned()));
    }
    let invalid = || ArtifactError::InvalidDescriptor(path.to_owned());
    let document = roxmltree::Document::parse_with_options(
        &input,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|_| invalid())?;
    let root = document.root_element();
    let Some(project_type) = designer_xml::project_type_from_root(root) else {
        return Ok(None);
    };
    let object = root
        .children()
        .find(roxmltree::Node::is_element)
        .ok_or_else(invalid)?;
    let uuid = object
        .attribute("uuid")
        .filter(|value| valid_uuid(value))
        .ok_or_else(invalid)?;
    let name = object
        .children()
        .find(|node| node.has_tag_name((NAMESPACE, "Properties")))
        .and_then(|properties| {
            properties
                .children()
                .find(|node| node.has_tag_name((NAMESPACE, "Name")))
        })
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)?;
    Ok(Some(Identity {
        project_type,
        uuid: uuid.to_ascii_lowercase(),
        name: name.to_owned(),
    }))
}

/// Compare UUIDs in a canonical spelling while rejecting missing or corrupt identities.
fn valid_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    /// Root identity ignores the filename, preserves names and normalizes UUID casing.
    #[test]
    fn reads_identity_and_rejects_ambiguous_or_corrupt_roots() {
        let fixture = TestDir::new();
        let path = fixture.0.join("Other.xml");
        let xml = concat!(
            "\u{feff}<m:MetaDataObject xmlns:m='http://v8.1c.ru/8.3/MDClasses'>",
            "<m:ExternalReport uuid='ABCDEF12-1234-1234-1234-123456789012'>",
            "<m:Properties><m:Name>Отчёт</m:Name></m:Properties></m:ExternalReport></m:MetaDataObject>"
        );
        fs::write(&path, xml).unwrap();
        let identity = inspect_identity(&fixture.0).unwrap().unwrap();
        assert_eq!(identity.project_type, ProjectType::Report);
        assert_eq!(identity.name, "Отчёт");
        assert_eq!(identity.uuid, "abcdef12-1234-1234-1234-123456789012");
        fs::write(fixture.0.join("Second.xml"), xml).unwrap();
        assert!(matches!(
            inspect_identity(&fixture.0),
            Err(ArtifactError::AmbiguousDescriptor(_))
        ));
        fs::remove_file(fixture.0.join("Second.xml")).unwrap();
        for invalid in [
            xml.replace("ABCDEF12-1234-1234-1234-123456789012", ""),
            "<!DOCTYPE x><x/>".to_owned(),
            "<broken>".to_owned(),
        ] {
            fs::write(&path, invalid).unwrap();
            assert!(matches!(
                inspect_identity(&fixture.0),
                Err(ArtifactError::InvalidDescriptor(_))
            ));
        }
    }
}
