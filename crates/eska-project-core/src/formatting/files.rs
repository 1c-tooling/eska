//! Source publication preflights the complete plan before replacing any user file.

use super::{
    Error,
    context::{Context, lock_error},
    protocol::Range,
    protocol::source_sha256,
};
use crate::project::source_lock::SourceLease;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
mod tests;

/// Exact before/after values remain in memory; a report or caller-supplied hash is not a write capability.
pub struct WritePlan {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
    pub range: Option<Range>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Applied,
    Failed,
    NotApplied,
}

/// IO failure after publication starts preserves explicit per-file outcomes, never a false all-or-nothing success.
pub struct Publication {
    pub states: Vec<State>,
    pub error: Option<Error>,
}

struct Prepared {
    path: PathBuf,
}

impl Drop for Prepared {
    /// Only this operation's newly created temporary file is removed on every return path.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Publish a bounded complete plan under shared source leases, comparing snapshots immediately before replacement.
#[must_use]
pub fn publish(context: &mut Context, plans: &[WritePlan], cancelled: &AtomicBool) -> Publication {
    let mut result = Publication {
        states: vec![State::NotApplied; plans.len()],
        error: None,
    };
    let prepared = prepare(context, plans, cancelled);
    let (temporary, leases) = match prepared {
        Ok(value) => value,
        Err(error) => {
            result.error = Some(error);
            return result;
        }
    };
    for (index, (plan, temporary)) in plans.iter().zip(&temporary).enumerate() {
        let operation = context
            .check_current()
            .and_then(|()| check(plan, cancelled))
            .and_then(|()| check_support(context, plan, leases.get(index)))
            .and_then(|()| {
                fs::rename(&temporary.path, &plan.path)
                    .map_err(|_| Error::new("source_write", plan.path.to_string_lossy()))
            });
        match operation {
            Ok(()) => result.states[index] = State::Applied,
            Err(error) => {
                result.states[index] = State::Failed;
                result.error = Some(error);
                break;
            }
        }
    }
    result
}

/// Acquire every lease and prepare every replacement before publishing the first source.
fn prepare(
    context: &mut Context,
    plans: &[WritePlan],
    cancelled: &AtomicBool,
) -> Result<(Vec<Prepared>, Vec<SourceLease>), Error> {
    context.check_current()?;
    let mut bytes = 0_usize;
    let mut paths = std::collections::BTreeSet::new();
    for plan in plans {
        bytes = bytes
            .saturating_add(plan.before.len())
            .saturating_add(plan.after.len());
        if bytes > 128 * 1024 * 1024 {
            return Err(Error::new("plan_too_large", "scope"));
        }
        if !paths.insert(&plan.path) {
            return Err(Error::new("invalid_plan", "duplicate_path"));
        }
        check(plan, cancelled)?;
        context
            .check_file(&plan.path)?
            .check_range(&plan.before, plan.range)?;
    }
    let mut leases = Vec::new();
    for plan in plans {
        leases.push(
            SourceLease::acquire(
                context.lock_root(&plan.path),
                std::slice::from_ref(&plan.path),
            )
            .map_err(|error| lock_error(&error, &plan.path))?,
        );
    }
    let mut temporary = Vec::new();
    for (index, plan) in plans.iter().enumerate() {
        check(plan, cancelled)?;
        check_support(context, plan, leases.get(index))?;
        temporary.push(write(plan)?);
    }
    Ok((temporary, leases))
}

/// Re-reading exact bytes also detects changes that arrived before the IDE watcher or CLI preflight.
fn check(plan: &WritePlan, cancelled: &AtomicBool) -> Result<(), Error> {
    if cancelled.load(Ordering::Acquire) {
        return Err(Error::new("cancelled", "operation"));
    }
    let metadata = fs::symlink_metadata(&plan.path)
        .map_err(|_| Error::new("source_io", plan.path.to_string_lossy()))?;
    if metadata.is_symlink()
        || !metadata.is_file()
        || fs::canonicalize(&plan.path).ok().as_ref() != Some(&plan.path)
    {
        return Err(Error::new("unsafe_path", plan.path.to_string_lossy()));
    }
    if metadata.permissions().readonly() {
        return Err(Error::new("source_read_only", plan.path.to_string_lossy()));
    }
    if metadata.len() != plan.before.len() as u64
        || fs::read(&plan.path).map_err(|_| Error::new("source_io", plan.path.to_string_lossy()))?
            != plan.before.as_bytes()
    {
        return Err(Error::new("source_conflict", plan.path.to_string_lossy()));
    }
    Ok(())
}

/// Same-directory atomic replacement retains permissions and never truncates an existing temporary name.
fn write(plan: &WritePlan) -> Result<Prepared, Error> {
    let directory = plan
        .path
        .parent()
        .ok_or_else(|| Error::new("unsafe_path", "path"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::new("source_write", "clock"))?
        .as_nanos();
    let hash = source_sha256(&plan.after);
    let path = directory.join(format!(
        ".eska-fmt-{}-{stamp}-{}.tmp",
        std::process::id(),
        &hash[..12]
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| Error::new("source_write", plan.path.to_string_lossy()))?;
    let temporary = Prepared { path };
    file.write_all(plan.after.as_bytes())
        .and_then(|()| file.set_permissions(fs::metadata(&plan.path)?.permissions()))
        .and_then(|()| file.sync_all())
        .map_err(|_| Error::new("source_write", plan.path.to_string_lossy()))?;
    Ok(temporary)
}

/// Expose preflight for previews without creating lock directories or writing temporary content.
///
/// # Errors
/// Returns stale source, support, permissions or owner conflicts before any publication.
pub fn preflight(
    context: &mut Context,
    path: &Path,
    before: &str,
    range: Option<Range>,
) -> Result<(), Error> {
    let plan = WritePlan {
        path: path.to_owned(),
        before: before.into(),
        after: String::new(),
        range,
    };
    check(&plan, &AtomicBool::new(false))?;
    context.check_file(path)?.check_range(before, range)
}

/// A late supplier-rule change cannot publish an earlier unrestricted formatting plan.
fn check_support(
    context: &mut Context,
    plan: &WritePlan,
    permit: Option<&SourceLease>,
) -> Result<(), Error> {
    context.check_support(&plan.path, permit)?.check_output(
        &plan.before,
        &plan.after,
        plan.range,
        plan.path
            .extension()
            .is_some_and(|extension| extension == "bsl"),
    )
}
