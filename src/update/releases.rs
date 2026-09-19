//! Stable crate release lookup with bounded HTTP responses.

use super::{Error, stable_version};
use semver::Version;
use serde::Deserialize;

#[derive(Deserialize)]
struct CrateResponse {
    versions: Vec<CrateVersion>,
}
#[derive(Deserialize)]
struct CrateVersion {
    num: String,
    yanked: bool,
}

/// Query crates.io rather than assuming GitHub and Cargo releases become available simultaneously.
pub(super) async fn cargo_version(
    client: &reqwest::Client,
    target: Option<&Version>,
) -> Result<Version, Error> {
    let mut response = client
        .get("https://crates.io/api/v1/crates/eska")
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
    let response: CrateResponse = serde_json::from_slice(bytes).map_err(|_| Error::Network)?;
    response
        .versions
        .into_iter()
        .filter(|value| !value.yanked)
        .filter_map(|value| stable_version(&value.num).ok())
        .filter(|value| target.is_none_or(|target| target == value))
        .max()
        .ok_or(Error::InvalidVersion)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skips_yanked_and_preview_releases_and_rejects_unpublished_targets() {
        let bytes = br#"{"versions":[{"num":"0.11.1","yanked":false},{"num":"0.12.0","yanked":true},{"num":"1.0.0-rc.1","yanked":false}]}"#;
        assert_eq!(select(bytes, None).unwrap().to_string(), "0.11.1");
        assert!(select(bytes, Some(&Version::new(0, 12, 0))).is_err());
        assert!(select(b"invalid", None).is_err());
    }
}
