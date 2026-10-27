//! Every syntax fragment and complete document share the same decoded output budget.

use eska_project_core::formatting::{Error, protocol::MAX_OUTPUT_BYTES};

/// Check before growing the destination, including appends of individually valid syntax units.
pub fn append(output: &mut String, text: &str) -> Result<(), Error> {
    if output.len().saturating_add(text.len()) > MAX_OUTPUT_BYTES {
        return Err(Error::new("output_too_large", "text"));
    }
    output.push_str(text);
    Ok(())
}
