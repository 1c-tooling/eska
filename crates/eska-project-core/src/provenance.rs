//! Portable repository provenance data supplied by an external VCS owner; no Git runtime.

use serde::{Deserialize, Deserializer, Serialize, de::Error};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Available,
    Absent,
    Unavailable,
}

/// Optional VCS data is captured by its owner before temporary build files are created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitProvenance {
    status: Status,
    commit: Option<String>,
    dirty: Option<bool>,
}

impl GitProvenance {
    /// Record a repository snapshot supplied by the VCS owner, including unborn HEAD.
    #[must_use]
    pub const fn available(commit: Option<String>, dirty: bool) -> Self {
        Self {
            status: Status::Available,
            commit,
            dirty: Some(dirty),
        }
    }

    /// Record a verified absence of repository metadata.
    #[must_use]
    pub const fn absent() -> Self {
        Self {
            status: Status::Absent,
            commit: None,
            dirty: None,
        }
    }

    /// Retain unknown provenance when an inspector was not requested or could not complete.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            status: Status::Unavailable,
            commit: None,
            dirty: None,
        }
    }
}

impl<'de> Deserialize<'de> for GitProvenance {
    /// Reject inconsistent external data while preserving the existing manifest JSON shape.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Document {
            status: Status,
            commit: Option<String>,
            dirty: Option<bool>,
        }
        let document = Document::deserialize(deserializer)?;
        match (document.status, document.commit, document.dirty) {
            (Status::Available, commit, Some(dirty)) => {
                if commit.as_ref().is_some_and(|value| {
                    !matches!(value.len(), 40 | 64)
                        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
                }) {
                    return Err(D::Error::custom("invalid Git object ID"));
                }
                Ok(Self::available(commit, dirty))
            }
            (Status::Absent, None, None) => Ok(Self::absent()),
            (Status::Unavailable, None, None) => Ok(Self::unavailable()),
            _ => Err(D::Error::custom("inconsistent Git provenance")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GitProvenance;

    /// Preserve every existing manifest state when crossing the process boundary.
    #[test]
    fn provenance_round_trips_without_changing_the_manifest_shape() {
        for (value, status) in [
            (GitProvenance::absent(), "absent"),
            (GitProvenance::unavailable(), "unavailable"),
            (GitProvenance::available(None, false), "available"),
            (
                GitProvenance::available(Some("a".repeat(40)), true),
                "available",
            ),
        ] {
            let json = serde_json::to_value(&value).unwrap();
            assert_eq!(json["status"], status);
            assert!(json.get("commit").is_some());
            assert!(json.get("dirty").is_some());
            assert_eq!(
                serde_json::from_value::<GitProvenance>(json).unwrap(),
                value
            );
        }
    }

    /// An unknown repository cannot be represented as a verified clean one by malformed IPC.
    #[test]
    fn malformed_provenance_is_rejected() {
        for json in [
            r#"{"status":"available","dirty":null}"#,
            r#"{"status":"absent","dirty":false}"#,
            r#"{"status":"unavailable","commit":"abc"}"#,
            r#"{"status":"available","commit":"abc","dirty":false}"#,
            r#"{"status":"unknown"}"#,
            r#"{"status":"absent","extra":true}"#,
        ] {
            assert!(
                serde_json::from_str::<GitProvenance>(json).is_err(),
                "{json}"
            );
        }
    }
}
