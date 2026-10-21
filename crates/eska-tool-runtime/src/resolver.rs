//! Deterministic component selection without executable probes or installation.

use crate::{Component, Error};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

/// Validate an explicit executable path without trying another installed component.
///
/// # Errors
/// Returns the explicit invalid path or filesystem failure.
pub fn resolve_at(component: Component, path: &Path) -> Result<PathBuf, Error> {
    let invalid = |source| Error::Invalid {
        component,
        path: path.to_owned(),
        source,
    };
    let canonical = fs::canonicalize(path).map_err(invalid)?;
    if !canonical.is_file() || env::current_exe().is_ok_and(|shell| shell == canonical) {
        return Err(Error::Selection {
            component,
            path: path.to_owned(),
        });
    }
    crate::installation::verify(&canonical, component.name())?;
    Ok(canonical)
}

/// Prefer an explicit path, then sibling release tools, PATH and the managed user bin directory.
///
/// # Errors
/// Reports a missing component or the first invalid selected executable without installing it.
pub fn resolve(component: Component) -> Result<PathBuf, Error> {
    if let Some(path) = env::var_os(component.variable()) {
        return resolve_at(component, Path::new(&path));
    }
    let name = format!("{}{}", component.name(), env::consts::EXE_SUFFIX);
    let mut candidates = Vec::new();
    if let Ok(shell) = env::current_exe()
        && let Some(directory) = shell.parent()
    {
        candidates.push(directory.join(&name));
    }
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path).map(|directory| directory.join(&name)));
    }
    if let Some(home) = env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        candidates.push(PathBuf::from(home).join(".eska/bin").join(name));
    }
    for candidate in candidates {
        match fs::symlink_metadata(&candidate) {
            Ok(_) => return resolve_at(component, &candidate),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(Error::Invalid {
                    component,
                    path: candidate,
                    source,
                });
            }
        }
    }
    Err(Error::Missing { component })
}
