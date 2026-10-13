//! Explicit, snapshot-bound edits for adopting the external-object version contract.

use std::ops::Range;

use tree_sitter::Node;

use super::{
    ContractError, ContractErrorKind, ParsedModule, ProjectVersion, descendants, error_at,
    inspect_contract, inspect_function, named_children, registration, version_function,
};

/// Describe an intentional source change without locale-dependent messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditKind {
    AddFunction,
    ExportFunction,
    BindRegistration,
}

impl EditKind {
    /// Return the stable plan spelling for CLI/IDE presentation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AddFunction => "add_function",
            Self::ExportFunction => "export_function",
            Self::BindRegistration => "bind_registration",
        }
    }
}

/// Original UTF-8 coordinates and the exact replacement shown in a migration preview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEdit {
    pub kind: EditKind,
    pub range: Range<usize>,
    pub replacement: String,
}

/// A plan can be applied only to the text that was inspected, with no implicit bump.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationPlan {
    original: String,
    version: ProjectVersion,
    edits: Vec<TextEdit>,
}

impl MigrationPlan {
    /// Expose the selected version without storing another independently editable value.
    #[must_use]
    pub const fn version(&self) -> &ProjectVersion {
        &self.version
    }

    /// Edits are ordered by their coordinates in the original module.
    #[must_use]
    pub fn edits(&self) -> &[TextEdit] {
        &self.edits
    }

    /// Apply only this previewed snapshot and revalidate the resulting contract.
    ///
    /// # Errors
    /// Refuses stale input or a result that cannot satisfy the syntax contract.
    pub fn apply(&self, input: &str) -> Result<String, ContractError> {
        if input != self.original {
            return Err(ContractError {
                kind: ContractErrorKind::SourceChanged,
                range: 0..input.len(),
            });
        }
        let mut output = input.to_owned();
        for edit in self.edits.iter().rev() {
            output.replace_range(edit.range.clone(), &edit.replacement);
        }
        inspect_contract(&output)?;
        Ok(output)
    }
}

/// Propose only missing-function creation, `Export` insertion and a proven BSP literal binding.
///
/// A missing function requires an explicit initial value. A suitable existing function keeps
/// its value; arbitrary bodies, registrations and conditional definitions are never rewritten.
/// No filesystem mutation occurs. Passing an empty module supports explicit source creation.
///
/// # Errors
/// Refuses invalid syntax, unmanaged definitions, missing/conflicting initial values or opaque BSP code.
pub fn plan_migration(
    input: &str,
    initial: Option<&ProjectVersion>,
) -> Result<MigrationPlan, ContractError> {
    let parsed = ParsedModule::parse(input)?;
    let nodes = descendants(parsed.tree.root_node());
    let mut edits = Vec::new();
    let version = if let Some(function) = version_function(&nodes, parsed.text, parsed.offset)? {
        let info = inspect_function(&parsed, false)?;
        if initial.is_some_and(|initial| initial != &info.version) {
            return Err(ContractError {
                kind: ContractErrorKind::InitialVersionConflict,
                range: info.value_range,
            });
        }
        if function.child_by_field_name("export").is_none() {
            let parameters = function
                .child_by_field_name("parameters")
                .ok_or_else(|| error_at(ContractErrorKind::Parameters, function, parsed.offset))?;
            let position = parameters.end_byte() + parsed.offset;
            let english = parsed.text[function.start_byte()..parameters.start_byte()]
                .to_ascii_lowercase()
                .starts_with("function");
            edits.push(TextEdit {
                kind: EditKind::ExportFunction,
                range: position..position,
                replacement: if english {
                    " Export"
                } else {
                    " Экспорт"
                }
                .to_owned(),
            });
        }
        info.version
    } else {
        let version = initial.ok_or(ContractError {
            kind: ContractErrorKind::InitialVersionRequired,
            range: input.len()..input.len(),
        })?;
        let position = insertion_position(&parsed)?;
        let prefix = &input[..position];
        let newline = if input.contains("\r\n") { "\r\n" } else { "\n" };
        let separator = if prefix.strip_prefix('\u{feff}').unwrap_or(prefix).is_empty() {
            String::new()
        } else if prefix.ends_with('\n') {
            newline.to_owned()
        } else {
            newline.repeat(2)
        };
        let function = [
            "// Возвращает версию внешнего объекта.",
            "//",
            "// Возвращаемое значение:",
            "//  Строка - номер версии проекта.",
            "Функция Версия() Экспорт",
            &format!("\tВозврат \"{version}\";"),
            "КонецФункции",
            "",
        ]
        .join(newline);
        edits.push(TextEdit {
            kind: EditKind::AddFunction,
            range: position..position,
            replacement: separator + &function,
        });
        version.clone()
    };
    if let Some(expression) = registration::version_expression(&nodes, parsed.text, parsed.offset)?
        && !registration::is_version_call(expression, parsed.text)
    {
        let literal = registration::literal_text(expression, parsed.text).ok_or_else(|| {
            error_at(
                ContractErrorKind::RegistrationUnsupported,
                expression,
                parsed.offset,
            )
        })?;
        ProjectVersion::parse(literal).map_err(|_| {
            error_at(
                ContractErrorKind::RegistrationUnsupported,
                expression,
                parsed.offset,
            )
        })?;
        edits.push(TextEdit {
            kind: EditKind::BindRegistration,
            range: expression.start_byte() + parsed.offset..expression.end_byte() + parsed.offset,
            replacement: "Версия()".to_owned(),
        });
    }
    edits.sort_by_key(|edit| edit.range.start);
    let plan = MigrationPlan {
        original: input.to_owned(),
        version,
        edits,
    };
    plan.apply(input)?;
    Ok(plan)
}

/// Place an unconditional declaration before initialization in every preprocessor branch.
///
/// Moving variables out of their conditional scope would change the module, so a mixed
/// declaration/initialization block has no automatic insertion point. Existing bytes stay put.
fn insertion_position(parsed: &ParsedModule<'_>) -> Result<usize, ContractError> {
    let children = named_children(parsed.tree.root_node());
    let Some(mut initialization) = children
        .into_iter()
        .find(|node| contains_initialization(*node))
    else {
        return Ok(parsed.input.len());
    };
    while initialization.kind() == "preprocessor" {
        let parts = named_children(initialization);
        if !parts
            .iter()
            .any(|node| node.kind() == "PREPROC_REGION_KEYWORD")
        {
            break;
        }
        let Some(child) = parts
            .into_iter()
            .find(|node| contains_initialization(*node))
        else {
            break;
        };
        initialization = child;
    }
    let position = initialization.start_byte();
    if let Some(variable) = descendants(parsed.tree.root_node())
        .into_iter()
        .find(|node| {
            node.start_byte() >= position
                && matches!(node.kind(), "var_definition" | "var_statement")
                && !inside_subroutine(*node)
        })
    {
        return Err(error_at(
            ContractErrorKind::InitializationConflict,
            variable,
            parsed.offset,
        ));
    }
    Ok(position + parsed.offset)
}

/// Skip subroutine bodies: their executable statements do not initialize the module.
fn contains_initialization(root: Node<'_>) -> bool {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if matches!(node.kind(), "function_definition" | "procedure_definition") {
            continue;
        }
        if node.kind().ends_with("_statement") && node.kind() != "var_statement" {
            return true;
        }
        pending.extend(named_children(node));
    }
    false
}

/// Local variable declarations remain part of their method and do not constrain insertion.
fn inside_subroutine(node: Node<'_>) -> bool {
    let mut parent = node.parent();
    while let Some(node) = parent {
        if matches!(node.kind(), "function_definition" | "procedure_definition") {
            return true;
        }
        parent = node.parent();
    }
    false
}

#[cfg(test)]
mod tests;
