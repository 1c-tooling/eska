//! Stable crate release lookup with bounded HTTP responses.

use super::{Error, stable_version};
use semver::Version;
use serde::Deserialize;

#[derive(Deserialize)]
struct CrateVersion {
    name: String,
    vers: String,
    v: Option<u32>,
    yanked: bool,
}

/// Query crates.io rather than assuming GitHub and Cargo releases become available simultaneously.
pub(super) async fn cargo_version(
    client: &reqwest::Client,
    target: Option<&Version>,
) -> Result<Version, Error> {
    let mut response = client
        .get("https://index.crates.io/es/ka/eska")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| Error::Network)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Network)? {
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
            return Err(Error::Network);
        }
        bytes.extend_from_slice(&chunk);
    }
    select(&bytes, target)
}

/// Select only published, non-yanked stable versions in this update channel.
fn select(bytes: &[u8], target: Option<&Version>) -> Result<Version, Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Network)?;
    let mut selected = None;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: CrateVersion = serde_json::from_str(line).map_err(|_| Error::Network)?;
        if value.name != "eska" || value.yanked || value.v.unwrap_or(1) > 2 {
            continue;
        }
        let Ok(version) = stable_version(&value.vers) else {
            continue;
        };
        if target.is_none_or(|target| target == &version)
            && selected.as_ref().is_none_or(|previous| previous < &version)
        {
            selected = Some(version);
        }
    }
    selected.ok_or(Error::InvalidVersion)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skips_yanked_and_preview_releases_and_rejects_unpublished_targets() {
        let bytes = br#"{"name":"eska","vers":"0.11.1","yanked":false}
{"name":"eska","vers":"0.12.0","yanked":true}
{"name":"eska","vers":"1.0.0-rc.1","yanked":false}
{"name":"eska","vers":"1.0.0","yanked":false,"v":3}
{"name":"other","vers":"2.0.0","yanked":false}
"#;
        assert_eq!(select(bytes, None).unwrap().to_string(), "0.11.1");
        assert!(select(bytes, Some(&Version::new(0, 12, 0))).is_err());
        assert!(select(b"invalid", None).is_err());
        assert!(select(b"", None).is_err());
        assert!(select(&[0xff], None).is_err());
        assert_eq!(
            select(bytes, Some(&Version::new(0, 11, 1)))
                .unwrap()
                .to_string(),
            "0.11.1"
        );
    }
}
