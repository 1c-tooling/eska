//! Shared gix network execution with explicit capability fallback.

use std::{fmt, path::Path, sync::atomic::AtomicBool};

use gix::bstr::ByteSlice;

use super::{command, repository::Repository};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityGap {
    RemoteHelper { scheme: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchBackend {
    Gix,
    SystemGit { reason: CapabilityGap },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOutcome {
    pub backend: FetchBackend,
}

#[derive(Debug)]
pub enum FetchError {
    Open(Box<gix::open::Error>),
    Remote(Box<gix::remote::find::existing::Error>),
    MissingUrl {
        remote: String,
    },
    Connect(Box<gix::remote::connect::Error>),
    Prepare(Box<gix::remote::fetch::prepare::Error>),
    Receive(Box<gix::remote::fetch::Error>),
    SystemGit {
        reason: CapabilityGap,
        source: command::Error,
    },
}

impl fmt::Display for FetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(source) => source.fmt(formatter),
            Self::Remote(source) => source.fmt(formatter),
            Self::MissingUrl { remote } => {
                write!(formatter, "remote {remote} has no fetch URL")
            }
            Self::Connect(source) => source.fmt(formatter),
            Self::Prepare(source) => source.fmt(formatter),
            Self::Receive(source) => source.fmt(formatter),
            Self::SystemGit { source, .. } => source.fmt(formatter),
        }
    }
}

impl std::error::Error for FetchError {}

/// Fetch one configured remote using gix or a preselected capability fallback.
///
/// Remote-helper transports are delegated before any gix network attempt because the pinned gix
/// transport rejects them. All other failures are returned directly and never retried with Git.
///
/// # Errors
/// Returns the exact open, remote, transport, fetch or system Git failure.
pub fn fetch(repository: &Repository, remote_name: &str) -> Result<FetchOutcome, FetchError> {
    let network_repository =
        gix::open(repository.git_dir()).map_err(|source| FetchError::Open(Box::new(source)))?;
    let remote = network_repository
        .find_remote(remote_name.as_bytes().as_bstr())
        .map_err(|source| FetchError::Remote(Box::new(source)))?;
    let url = remote
        .url(gix::remote::Direction::Fetch)
        .ok_or_else(|| FetchError::MissingUrl {
            remote: remote_name.to_owned(),
        })?;
    let backend = fetch_backend(url);

    if let FetchBackend::SystemGit { reason } = &backend {
        command::Executor::new(repository.work_dir())
            .fetch(remote_name)
            .map_err(|source| FetchError::SystemGit {
                reason: reason.clone(),
                source,
            })?;
        return Ok(FetchOutcome { backend });
    }

    let connection = remote
        .connect(gix::remote::Direction::Fetch)
        .map_err(|source| FetchError::Connect(Box::new(source)))?;
    let prepare = connection
        .prepare_fetch(
            gix::progress::Discard,
            gix::remote::ref_map::Options::default(),
        )
        .map_err(|source| FetchError::Prepare(Box::new(source)))?;
    let interrupt = AtomicBool::new(false);
    prepare
        .receive(gix::progress::Discard, &interrupt)
        .map_err(|source| FetchError::Receive(Box::new(source)))?;
    Ok(FetchOutcome { backend })
}

/// Select the network backend solely from a transport capability known before execution.
fn fetch_backend(url: &gix::Url) -> FetchBackend {
    match &url.scheme {
        gix::url::Scheme::Ext | gix::url::Scheme::Helper(_) | gix::url::Scheme::HelperUrl(_) => {
            FetchBackend::SystemGit {
                reason: CapabilityGap::RemoteHelper {
                    scheme: url.scheme.as_str().to_owned(),
                },
            }
        }
        _ => FetchBackend::Gix,
    }
}

#[derive(Debug)]
pub enum CloneError {
    Prepare(Box<gix::clone::Error>),
    RemoteName(gix::remote::name::Error),
    Fetch(Box<gix::clone::fetch::Error>),
    Checkout(Box<gix::clone::checkout::main_worktree::Error>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloneOutcome {
    pub collisions: usize,
    pub errors: usize,
}

/// Clone, fetch and check out one repository through the shared gix network boundary.
///
/// # Errors
/// Returns the stage-specific gix clone, fetch or checkout failure.
pub fn clone_checkout(
    repository: gix::Url,
    destination: &Path,
    remote_name: &str,
) -> Result<CloneOutcome, CloneError> {
    let mut prepare = gix::prepare_clone(repository, destination)
        .map_err(|source| CloneError::Prepare(Box::new(source)))?
        .with_remote_name(remote_name.as_bytes())
        .map_err(CloneError::RemoteName)?;
    let interrupt = AtomicBool::new(false);
    let (mut checkout, _) = prepare
        .fetch_then_checkout(gix::progress::Discard, &interrupt)
        .map_err(|source| CloneError::Fetch(Box::new(source)))?;
    let (_repository, outcome) = checkout
        .main_worktree(gix::progress::Discard, &interrupt)
        .map_err(|source| CloneError::Checkout(Box::new(source)))?;
    Ok(CloneOutcome {
        collisions: outcome.collisions.len(),
        errors: outcome.errors.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_helpers_select_a_structured_system_git_fallback() {
        let url = gix::Url::try_from("test::repository").expect("remote-helper URL");

        assert_eq!(
            fetch_backend(&url),
            FetchBackend::SystemGit {
                reason: CapabilityGap::RemoteHelper {
                    scheme: "test".to_owned(),
                },
            }
        );
    }

    #[test]
    fn supported_transports_never_select_fallback() {
        for address in [
            "/tmp/repository.git",
            "file:///tmp/repository.git",
            "ssh://example.invalid/repository.git",
            "git://example.invalid/repository.git",
            "https://example.invalid/repository.git",
        ] {
            let url = gix::Url::try_from(address).expect("supported URL");
            assert_eq!(fetch_backend(&url), FetchBackend::Gix, "{address}");
        }
    }
}
