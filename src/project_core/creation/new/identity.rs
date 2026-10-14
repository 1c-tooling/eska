//! Fallible system entropy produces native UUIDs without a second identity wrapper.

use crate::project_core::source_format::{Error, NativeUuid};

/// Generate RFC version-4 and variant-1 bits using the operating system's random source.
pub(super) fn fresh() -> Result<NativeUuid, Error> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::root("source_entropy_unavailable"))?;
    from_bytes(bytes)
}

/// Formatting is architecture-independent, and the existing native wrapper checks the final spelling.
fn from_bytes(mut bytes: [u8; 16]) -> Result<NativeUuid, Error> {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let value = u128::from_be_bytes(bytes);
    NativeUuid::parse(&format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        value >> 96,
        (value >> 80) & 0xffff,
        (value >> 64) & 0xffff,
        (value >> 48) & 0xffff,
        value & 0xffff_ffff_ffff,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All-zero and all-one entropy retain their random bits except the mandated RFC marker bits.
    #[test]
    fn rfc_uuid_markers_are_independent_of_input_bits() {
        assert_eq!(
            from_bytes([0; 16]).unwrap().as_str(),
            "00000000-0000-4000-8000-000000000000"
        );
        assert_eq!(
            from_bytes([255; 16]).unwrap().as_str(),
            "ffffffff-ffff-4fff-bfff-ffffffffffff"
        );
    }
}
