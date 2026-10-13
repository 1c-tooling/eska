//! Native query bindings and reversible text slots are independent of project paths and permissions.

use std::{collections::BTreeMap, ops::Range};

use super::{Error, FormId};

mod native;
#[cfg(test)]
mod reference_tests;
mod references;
mod stencil;
#[cfg(test)]
mod tests;

pub use references::{
    ExtractedQuerySource, QUERY_REFERENCE_NAMESPACE, QueryReference, QueryReferenceDocument,
};

/// Dataset names are scoped by their containing union; form attribute IDs have a separate native scope.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum QuerySlot {
    Composition { datasets: Vec<String> },
    DynamicList { attribute: FormId },
}

/// A decoded query has one text value and an immutable native inverse slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeQuery {
    pub slot: QuerySlot,
    pub text: String,
    frame: stencil::Frame,
}

impl NativeQuery {
    /// Native coordinates belong to this exact input document, without project or policy authority.
    #[must_use]
    pub fn native_range(&self) -> Range<usize> {
        self.frame.range.clone()
    }
}

/// XML gaps contain no extracted query values; inverse frames retain transport rather than another text copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeQueryDocument {
    parts: Vec<String>,
    queries: Vec<NativeQuery>,
}

impl NativeQueryDocument {
    /// Read known DCS datasets or saved manual dynamic-list queries without interpreting their language.
    ///
    /// # Errors
    /// Returns invalid XML, ambiguous native scopes or an unverified profile/lexical inverse.
    pub fn from_xml(input: &str) -> Result<Self, Error> {
        native::decode(input)
    }

    /// Queries retain native order; filenames and XML positions never substitute for their scoped bindings.
    #[must_use]
    pub fn queries(&self) -> &[NativeQuery] {
        &self.queries
    }

    /// Reconstruct original XML using the sole decoded values and independently preserved gaps.
    ///
    /// # Errors
    /// Returns a binding, XML character or lexical materialization failure.
    pub fn to_xml(&self) -> Result<String, Error> {
        self.render(|query| Some(query.text.as_str()))
    }

    /// Materialize exactly the declared query slots; missing or extra values cannot activate an XML fallback.
    ///
    /// # Errors
    /// Returns missing/extra bindings, invalid XML characters or a failed native reparse.
    pub fn materialize(&self, values: &BTreeMap<QuerySlot, String>) -> Result<String, Error> {
        if values.len() != self.queries.len()
            || self
                .queries
                .iter()
                .any(|query| !values.contains_key(&query.slot))
        {
            return Err(binding());
        }
        self.render(|query| values.get(&query.slot).map(String::as_str))
    }

    /// Both callers borrow their sole selected values through the same checked inverse writer.
    fn render<'a>(
        &'a self,
        value: impl Fn(&'a NativeQuery) -> Option<&'a str>,
    ) -> Result<String, Error> {
        let mut output = String::new();
        for (part, query) in self.parts.iter().zip(&self.queries) {
            output.push_str(part);
            let text = value(query).ok_or_else(binding)?;
            super::validation::xml_text(text)?;
            output.push_str(&query.frame.render(text));
        }
        output.push_str(self.parts.last().ok_or_else(binding)?);
        let checked = Self::from_xml(&output)?;
        if checked.queries.len() != self.queries.len()
            || checked
                .queries
                .iter()
                .zip(&self.queries)
                .any(|(actual, expected)| {
                    actual.slot != expected.slot || value(expected) != Some(actual.text.as_str())
                })
        {
            return Err(binding());
        }
        Ok(output)
    }
}

/// Native bindings are validated before any project transport or supplier permission is selected.
fn binding() -> Error {
    Error::root("source_query_binding_invalid")
}

/// Preserved foreign/unknown XML is not silently promoted to editable query language.
fn unverified() -> Error {
    Error::root("source_query_profile_unverified")
}
