//! Approved workspace pins have a versioned strict document independent of editable manifests.

use super::{AcceptedRef, BTreeMap, Error, ProjectName, Workspace};
use serde::{Deserialize, Deserializer, de::MapAccess};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    #[serde(deserialize_with = "unique_members")]
    members: BTreeMap<String, String>,
}

/// A complete per-member trust selection; no global environment pin is reused implicitly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceAcceptedRefs {
    members: BTreeMap<ProjectName, AcceptedRef>,
}

impl WorkspaceAcceptedRefs {
    /// Parse schema 1, refusing duplicate names, unknown fields and invalid names/checksums.
    ///
    /// # Errors
    /// Returns `source_workspace_pins_invalid` for malformed or unsupported pin documents.
    pub fn from_json(input: &str) -> Result<Self, Error> {
        let invalid = || Error::root("source_workspace_pins_invalid");
        if input.len() > 1024 * 1024 {
            return Err(invalid());
        }
        let document: Document = serde_json::from_str(input).map_err(|_| invalid())?;
        if document.schema_version != 1 {
            return Err(invalid());
        }
        let members = document
            .members
            .into_iter()
            .map(|(name, sha256)| {
                let name = ProjectName::parse(name).map_err(|_| invalid())?;
                if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(invalid());
                }
                Ok((
                    name,
                    AcceptedRef {
                        sha256: sha256.to_ascii_lowercase(),
                    },
                ))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self { members })
    }

    /// Retrieve an explicitly supplied checksum without guessing by project path or native UUID.
    #[must_use]
    pub fn reference(&self, name: &ProjectName) -> Option<&AcceptedRef> {
        self.members.get(name)
    }

    /// Missing and extra pins both refuse the entire workspace before any destination is created.
    pub(super) fn check_members(&self, workspace: &Workspace) -> Result<(), Error> {
        if workspace.members().len() != self.members.len()
            || workspace
                .members()
                .iter()
                .any(|member| !self.members.contains_key(member.name()))
        {
            return Err(Error::root("source_workspace_pins_mismatch"));
        }
        Ok(())
    }
}

/// A repeated JSON key is ambiguous trust input, even if both copies contain the same checksum.
fn unique_members<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Members;
    impl<'de> serde::de::Visitor<'de> for Members {
        type Value = BTreeMap<String, String>;

        /// Describe the stable wire type for serde's non-localized parsing failure.
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a mapping of unique member names to SHA-256 checksums")
        }

        /// Preserve uniqueness before collecting the caller's approved member selection.
        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut members = BTreeMap::new();
            while let Some((name, pin)) = map.next_entry::<String, String>()? {
                if members.insert(name, pin).is_some() {
                    return Err(serde::de::Error::custom("duplicate member"));
                }
            }
            Ok(members)
        }
    }
    deserializer.deserialize_map(Members)
}
