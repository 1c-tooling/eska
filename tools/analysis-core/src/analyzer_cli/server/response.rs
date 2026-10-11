//! Bound serialized bytes before the transport writes any part of a response frame.

use std::io::{self, Write};

use serde_json::{Value, json};

use super::protocol;

/// Oversized responses become bounded protocol errors with the same request ID and current revision.
pub(super) fn encode(
    response: &Value,
    snapshot: Option<&Value>,
    limit: usize,
) -> serde_json::Result<Vec<u8>> {
    let mut body = Vec::new();
    if serialize(response, &mut body, limit).is_err() {
        let error = json!({"jsonrpc":"2.0", "id":response["id"],
            "error":protocol::error_value(-32000,"response_too_large",
                json!({"limit":limit,"snapshot":snapshot}))});
        serialize(&error, &mut body, limit)?;
    }
    Ok(body)
}

/// Reuse the buffer for late cancellation or overflow without retaining bytes from a prior attempt.
pub(super) fn serialize(
    response: &Value,
    body: &mut Vec<u8>,
    limit: usize,
) -> serde_json::Result<()> {
    body.clear();
    serde_json::to_writer(LimitedBody { body, limit }, response)
}

struct LimitedBody<'a> {
    body: &'a mut Vec<u8>,
    limit: usize,
}

impl Write for LimitedBody<'_> {
    /// Serialization never grows the wire buffer beyond the advertised response cap.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.body.len()) {
            return Err(io::Error::other("response_too_large"));
        }
        self.body.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// The in-memory buffer has no deferred output.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
