//! Current call binding owns overload/argument selection; the adapter validates display spans only.

#[cfg(test)]
mod tests;

use std::ops::Range;

use eska::project_core::source_format::SourcePath;

use crate::{AnalysisSession, ModuleResult, QueryCancellation, QueryError, SessionRevision};

use super::definitions::validate_offset;

/// A parameter span is relative to its generated signature label, never a source-file location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallParameter {
    pub label: String,
    pub documentation: Option<String>,
    pub byte_range: Range<u32>,
}

/// Labels are the pinned kernel's BSL signature display; authored purpose text is not UI translation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallSignature {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<CallParameter>,
}

/// Candidate order and active indices are selected by the same kernel call binding as inference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallSignatures {
    pub signatures: Vec<CallSignature>,
    pub active_signature: Option<usize>,
    pub active_parameter: Option<usize>,
}

impl AnalysisSession {
    /// Resolve signatures at an exact current call-site cursor, without shifting it to an identifier.
    ///
    /// # Errors
    /// Returns stale/unread input, invalid offset, cancellation or inconsistent kernel display data.
    pub fn module_signature_help(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        cancellation: &QueryCancellation,
    ) -> Result<ModuleResult<Option<CallSignatures>>, QueryError> {
        self.query_module(path, expected, cancellation, |analysis, file| {
            validate_offset(analysis, file, path, offset)?;
            let Some(help) = analysis.signature_help(file, offset) else {
                return Ok(None);
            };
            if help.signatures.is_empty() {
                return Ok(None);
            }
            project(help, cancellation).map(Some)
        })
    }
}

/// Retain native candidates and selection rather than ranking or guessing missing arguments locally.
fn project(
    help: ide::SignatureHelp,
    control: &QueryCancellation,
) -> Result<CallSignatures, QueryError> {
    let selected = help.active_signature.unwrap_or(0);
    let signature = help
        .signatures
        .get(selected)
        .ok_or(QueryError::InconsistentSignatureHelp {
            signature: selected,
            parameter: help.active_parameter,
        })?;
    if help
        .active_parameter
        .is_some_and(|index| index >= signature.parameters.len())
    {
        return Err(QueryError::InconsistentSignatureHelp {
            signature: selected,
            parameter: help.active_parameter,
        });
    }
    let signatures = help
        .signatures
        .into_iter()
        .enumerate()
        .map(|(index, signature)| project_signature(index, signature, control))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CallSignatures {
        signatures,
        active_signature: help.active_signature,
        active_parameter: help.active_parameter,
    })
}

/// Native labels contain the ordered rendered parameters joined by comma/space inside parentheses.
fn project_signature(
    index: usize,
    signature: ide::SignatureInformation,
    control: &QueryCancellation,
) -> Result<CallSignature, QueryError> {
    if control.is_cancelled() {
        return Err(QueryError::Cancelled);
    }
    let invalid = |parameter| QueryError::InconsistentSignatureHelp {
        signature: index,
        parameter,
    };
    let mut cursor = signature.signature.find('(').ok_or_else(|| invalid(None))? + 1;
    let mut parameters = Vec::with_capacity(signature.parameters.len());
    let parameter_count = signature.parameters.len();
    for (parameter, info) in signature.parameters.into_iter().enumerate() {
        if control.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        if info.label.is_empty()
            || !signature
                .signature
                .get(cursor..)
                .is_some_and(|tail| tail.starts_with(&info.label))
        {
            return Err(invalid(Some(parameter)));
        }
        let end = cursor
            .checked_add(info.label.len())
            .ok_or_else(|| invalid(Some(parameter)))?;
        let range = u32::try_from(cursor)
            .and_then(|start| u32::try_from(end).map(|end| start..end))
            .map_err(|_| invalid(Some(parameter)))?;
        parameters.push(CallParameter {
            label: info.label,
            documentation: info.documentation,
            byte_range: range,
        });
        cursor = end;
        if parameter + 1 < parameter_count {
            if !signature
                .signature
                .get(cursor..)
                .is_some_and(|tail| tail.starts_with(", "))
            {
                return Err(invalid(Some(parameter)));
            }
            cursor += 2;
        }
    }
    if !signature
        .signature
        .get(cursor..)
        .is_some_and(|tail| tail.starts_with(')'))
    {
        return Err(invalid(None));
    }
    Ok(CallSignature {
        label: signature.signature,
        documentation: signature.doc,
        parameters,
    })
}
