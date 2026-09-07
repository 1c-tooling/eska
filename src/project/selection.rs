//! Locale-independent selection of projects from a discovery context.

use super::{Project, ProjectName, ProjectNameError, discovery::DiscoveryContext};

/// Whether an operation can safely target multiple workspace members.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionIntent {
    ReadOnly,
    SingleMutation,
}

/// One project selected from a standalone or workspace context.
#[derive(Clone, Copy, Debug)]
pub struct SelectedProject<'a> {
    name: Option<&'a ProjectName>,
    project: &'a Project,
}

/// Resolved projects plus the output shape implied by the selectors and context.
#[derive(Clone, Debug)]
pub struct ProjectSelection<'a> {
    projects: Vec<SelectedProject<'a>>,
    aggregate: bool,
}

impl<'a> ProjectSelection<'a> {
    #[must_use]
    pub fn projects(&self) -> &[SelectedProject<'a>] {
        &self.projects
    }

    #[must_use]
    pub const fn is_aggregate(&self) -> bool {
        self.aggregate
    }
}

impl<'a> SelectedProject<'a> {
    #[must_use]
    pub const fn name(self) -> Option<&'a ProjectName> {
        self.name
    }

    #[must_use]
    pub const fn project(self) -> &'a Project {
        self.project
    }
}

/// Resolve CLI-facing selector values against a validated discovery context.
///
/// # Errors
/// Returns a structured error for invalid, duplicate, unknown, incompatible, or
/// insufficiently specific selectors.
pub fn select_projects<'a>(
    context: &'a DiscoveryContext,
    requested_names: &[String],
    entire_workspace: bool,
    intent: SelectionIntent,
) -> Result<ProjectSelection<'a>, SelectionError> {
    let names = parse_names(requested_names)?;
    match context {
        DiscoveryContext::Standalone(project) => {
            if entire_workspace || !names.is_empty() {
                return Err(SelectionError::WorkspaceSelectorForStandalone);
            }
            Ok(ProjectSelection {
                projects: vec![SelectedProject {
                    name: None,
                    project,
                }],
                aggregate: false,
            })
        }
        DiscoveryContext::Workspace {
            workspace,
            current_member,
        } => {
            if entire_workspace && !names.is_empty() {
                return Err(SelectionError::ConflictingSelectors);
            }
            if intent == SelectionIntent::SingleMutation {
                return select_single(workspace, current_member.as_ref(), &names, entire_workspace);
            }
            if !names.is_empty() {
                let projects = names
                    .iter()
                    .map(|name| select_named(workspace, name))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(ProjectSelection {
                    aggregate: projects.len() != 1,
                    projects,
                });
            }
            if !entire_workspace && let Some(current) = current_member {
                return select_named(workspace, current).map(|project| ProjectSelection {
                    projects: vec![project],
                    aggregate: false,
                });
            }
            Ok(ProjectSelection {
                projects: workspace
                    .members()
                    .iter()
                    .map(|member| SelectedProject {
                        name: Some(member.name()),
                        project: member.project(),
                    })
                    .collect(),
                aggregate: true,
            })
        }
    }
}

fn parse_names(values: &[String]) -> Result<Vec<ProjectName>, SelectionError> {
    let mut names = Vec::with_capacity(values.len());
    for value in values {
        let name = ProjectName::parse(value.clone()).map_err(SelectionError::InvalidName)?;
        if names.contains(&name) {
            return Err(SelectionError::DuplicateSelector { name });
        }
        names.push(name);
    }
    Ok(names)
}

fn select_single<'a>(
    workspace: &'a super::Workspace,
    current_member: Option<&ProjectName>,
    names: &[ProjectName],
    entire_workspace: bool,
) -> Result<ProjectSelection<'a>, SelectionError> {
    if entire_workspace || names.len() > 1 {
        return Err(SelectionError::SingleProjectRequired);
    }
    if let Some(name) = names.first() {
        return select_named(workspace, name).map(|project| ProjectSelection {
            projects: vec![project],
            aggregate: false,
        });
    }
    let current_member = current_member.ok_or(SelectionError::ExplicitProjectRequired)?;
    select_named(workspace, current_member).map(|project| ProjectSelection {
        projects: vec![project],
        aggregate: false,
    })
}

fn select_named<'a>(
    workspace: &'a super::Workspace,
    name: &ProjectName,
) -> Result<SelectedProject<'a>, SelectionError> {
    workspace
        .member(name)
        .map(|member| SelectedProject {
            name: Some(member.name()),
            project: member.project(),
        })
        .ok_or_else(|| SelectionError::UnknownProject { name: name.clone() })
}

/// A structured project selector failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionError {
    InvalidName(ProjectNameError),
    DuplicateSelector { name: ProjectName },
    UnknownProject { name: ProjectName },
    WorkspaceSelectorForStandalone,
    ConflictingSelectors,
    ExplicitProjectRequired,
    SingleProjectRequired,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{SelectionError, SelectionIntent, select_projects};
    use crate::project::{
        Project, ProjectConfiguration, ProjectName, ProjectType, SourceFormat, Workspace,
        WorkspaceMember, build::BuildSettings, discovery::DiscoveryContext,
    };

    fn context(current: Option<&str>) -> DiscoveryContext {
        let root = PathBuf::from("/workspace");
        let members = ["report", "processing"]
            .into_iter()
            .map(|name| {
                let member_root = root.join(name);
                WorkspaceMember::new(
                    ProjectName::parse(name.to_owned()).unwrap(),
                    Project::new(
                        member_root.clone(),
                        member_root,
                        ProjectConfiguration::new(ProjectType::Report, SourceFormat::DesignerXml),
                    )
                    .unwrap(),
                )
            })
            .collect();
        DiscoveryContext::Workspace {
            workspace: Workspace::new(root, BuildSettings::default(), None, members),
            current_member: current.map(|name| ProjectName::parse(name.to_owned()).unwrap()),
        }
    }

    #[test]
    fn read_only_defaults_to_all_at_root_and_current_inside_member() {
        assert_eq!(
            select_projects(&context(None), &[], false, SelectionIntent::ReadOnly)
                .unwrap()
                .projects()
                .len(),
            2
        );
        let member = context(Some("processing"));
        let selected = select_projects(&member, &[], false, SelectionIntent::ReadOnly).unwrap();
        assert_eq!(
            selected.projects()[0].name().unwrap().as_str(),
            "processing"
        );
    }

    #[test]
    fn mutation_at_workspace_root_requires_one_explicit_name() {
        assert_eq!(
            select_projects(&context(None), &[], false, SelectionIntent::SingleMutation)
                .unwrap_err(),
            SelectionError::ExplicitProjectRequired
        );
        assert!(matches!(
            select_projects(
                &context(None),
                &["missing".to_owned()],
                false,
                SelectionIntent::SingleMutation
            ),
            Err(SelectionError::UnknownProject { .. })
        ));
    }
}
