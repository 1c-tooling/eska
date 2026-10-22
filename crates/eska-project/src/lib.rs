//! Project command presentation shared by the standalone tool and the existing ESKA CLI.

pub mod doctor;
pub mod ide;
pub mod metadata;
pub mod onboarding;
pub mod version;

mod analyzer_config;
mod cli;
mod context;
mod operations;
mod response;

pub use cli::run;

#[cfg(test)]
#[path = "../../../tests/support/mod.rs"]
mod test_support;

/// Bind rollback unit fixtures to the component built outside Cargo's `deps` directory.
#[cfg(test)]
fn test_component(component: eska_tool_runtime::Component) -> std::path::PathBuf {
    std::env::current_exe()
        .expect("test executable")
        .parent()
        .and_then(std::path::Path::parent)
        .expect("Cargo target directory")
        .join(format!(
            "{}{}",
            component.name(),
            std::env::consts::EXE_SUFFIX
        ))
}
