//! Shared Designer XML project model, discovery and checked metadata operations.
//!
//! This crate contains no Git repository, platform runner or language-server runtime.

pub mod doctor;
pub mod formatting;

pub mod application;
pub mod artifact;
pub mod config;
pub mod patch;
pub mod project;
pub mod provenance;
pub mod workflow;

#[cfg(test)]
#[path = "../../../tests/support/mod.rs"]
mod test_support;

/// Attach the current Windows drive to rooted virtual fixtures without normalizing `..` away.
#[cfg(test)]
fn test_path(value: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(value);
    #[cfg(windows)]
    if path.has_root() && !path.is_absolute() {
        let current = std::env::current_dir().expect("test working directory");
        let prefix = current.components().next().expect("native drive prefix");
        return std::path::PathBuf::from(prefix.as_os_str()).join(path);
    }
    path.to_owned()
}
