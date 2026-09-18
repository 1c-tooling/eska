//! Bounded byte framing; malformed boundaries never trigger stream resynchronization.

use std::io::{self, BufRead, Write};

pub(super) const MAX_HEADER: usize = 8192;
pub(super) const MAX_REQUEST: usize = 1_048_576;
pub(super) const MAX_RESPONSE: usize = 67_108_864;

/// Read exactly one frame, distinguishing clean EOF from a truncated header or body.
pub(super) fn read_frame(input: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut header = Vec::new();
    loop {
        let mut byte = [0];
        if input.read(&mut byte)? == 0 {
            return if header.is_empty() {
                Ok(None)
            } else {
                Err(invalid())
            };
        }
        header.push(byte[0]);
        if header.len() > MAX_HEADER || !byte[0].is_ascii() {
            return Err(invalid());
        }
        if byte[0] == b'\n' && !header.ends_with(b"\r\n") {
            return Err(invalid());
        }
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let text = std::str::from_utf8(&header).map_err(|_| invalid())?;
    let mut length = None;
    for line in text[..text.len() - 4].split("\r\n") {
        let (key, value) = line.split_once(':').ok_or_else(invalid)?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || !value
                .bytes()
                .all(|byte| byte == b' ' || byte == b'\t' || byte.is_ascii_graphic())
        {
            return Err(invalid());
        }
        let value = value.trim();
        if key.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid());
            }
            length = Some(value.parse::<usize>().map_err(|_| invalid())?);
        } else if key.eq_ignore_ascii_case("Content-Type")
            && !value.eq_ignore_ascii_case("application/vscode-jsonrpc; charset=utf-8")
        {
            return Err(invalid());
        }
    }
    let length = length
        .filter(|n| (1..=MAX_REQUEST).contains(n))
        .ok_or_else(invalid)?;
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    if body.starts_with(&[0xef, 0xbb, 0xbf]) || std::str::from_utf8(&body).is_err() {
        return Err(invalid());
    }
    Ok(Some(body))
}

/// Write a complete frame under the caller's single-writer ownership.
pub(super) fn write_frame(output: &mut impl Write, body: &[u8]) -> io::Result<()> {
    if body.is_empty() || body.len() > MAX_RESPONSE {
        return Err(invalid());
    }
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(body)?;
    output.flush()
}

/// Deliberately omit untrusted input from transport errors.
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid IDE frame")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor, Read};

    /// A reader limiting every read to one byte models arbitrary pipe fragmentation.
    struct Bytewise(Cursor<Vec<u8>>);
    impl Read for Bytewise {
        /// Limit each underlying read without changing the framing implementation.
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let length = out.len().min(1);
            self.0.read(&mut out[..length])
        }
    }

    /// Byte counts preserve Cyrillic and concatenated frames across one-byte reads.
    #[test]
    fn fragmented_and_coalesced_frames() {
        let body = "{\"name\":\"Контрагенты\"}".as_bytes();
        let mut wire = Vec::new();
        write_frame(&mut wire, body).unwrap();
        write_frame(&mut wire, b"{}").unwrap();
        let mut reader = BufReader::new(Bytewise(Cursor::new(wire)));
        assert_eq!(read_frame(&mut reader).unwrap().unwrap(), body);
        assert_eq!(read_frame(&mut reader).unwrap().unwrap(), b"{}");
        assert!(read_frame(&mut reader).unwrap().is_none());
    }

    /// Invalid lengths, encodings, headers and partial frames are terminal.
    #[test]
    fn rejects_invalid_boundaries() {
        for wire in [
            b"Content-Length: 2\n\n{}".as_slice(),
            b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
            b"Content-Length: 0\r\n\r\n",
            b"Content-Length: +2\r\n\r\n{}",
            b"Content-Length: 1048577\r\n\r\n",
            b"Content-Length: 2\r\n\r\n{",
            b"Content-Length: 1\r\n\r\n\xff",
            b"Content-Length: 3\r\n\r\n\xef\xbb\xbf",
            b"Content-Length: 2\r\nContent-Type: application/json\r\n\r\n{}",
        ] {
            assert!(read_frame(&mut Cursor::new(wire)).is_err());
        }
        let oversized = vec![b'A'; MAX_HEADER + 1];
        assert!(read_frame(&mut Cursor::new(oversized)).is_err());
    }
}
