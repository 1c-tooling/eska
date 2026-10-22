//! Dependency ordering is stable by declaration order and never merges separate applications.

use std::collections::{BTreeMap, BTreeSet};

use crate::project::ProjectName;

use super::{Application, SettingsError};

/// Use iterative topological sorting so malformed large graphs cannot exhaust the stack.
pub(super) fn ordered_extensions(
    name: &ProjectName,
    application: &Application,
) -> Result<Vec<ProjectName>, SettingsError> {
    let extensions = application.extensions();
    let positions: BTreeMap<_, _> = extensions
        .iter()
        .enumerate()
        .map(|(index, name)| (name, index))
        .collect();
    let mut incoming = vec![0usize; extensions.len()];
    let mut dependants = vec![Vec::new(); extensions.len()];
    for (member, dependencies) in application.extension_dependencies() {
        let owner = position(name, &positions, member)?;
        let mut unique = BTreeSet::new();
        for dependency in dependencies {
            let target = position(name, &positions, dependency)?;
            if !unique.insert(target) {
                return Err(SettingsError::DuplicateMember {
                    application: name.clone(),
                    member: dependency.clone(),
                });
            }
            incoming[owner] += 1;
            dependants[target].push(owner);
        }
    }
    let mut ready: BTreeSet<_> = incoming
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count == 0).then_some(index))
        .collect();
    let mut ordered = Vec::with_capacity(extensions.len());
    while let Some(index) = ready.pop_first() {
        ordered.push(extensions[index].clone());
        for dependant in &dependants[index] {
            incoming[*dependant] -= 1;
            if incoming[*dependant] == 0 {
                ready.insert(*dependant);
            }
        }
    }
    if ordered.len() != extensions.len() {
        return Err(SettingsError::DependencyCycle {
            application: name.clone(),
        });
    }
    Ok(ordered)
}

/// Each dependency edge must refer to a selected extension, including the edge's owner.
fn position(
    application: &ProjectName,
    positions: &BTreeMap<&ProjectName, usize>,
    member: &ProjectName,
) -> Result<usize, SettingsError> {
    positions
        .get(member)
        .copied()
        .ok_or_else(|| SettingsError::DependencyOutsideApplication {
            application: application.clone(),
            member: member.clone(),
        })
}
