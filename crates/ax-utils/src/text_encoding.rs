//! Read source files as text — UTF-8, BOM variants, and UTF-16 (Windows/Cursor).

use std::path::Path;

use crate::errors::FileError;

/// Read a project file as Unicode text, handling common encodings on Windows.
pub fn read_text_file(path: &Path) -> Result<String, FileError> {
    let bytes = std::fs::read(path)
        .map_err(|e| FileError::with_path(e.to_string(), path.display().to_string()))?;
    decode_text_bytes(&bytes).map_err(|message| FileError::with_path(message, path.display().to_string()))
}

fn decode_text_bytes(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }

    // UTF-8 BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return utf8_slice(&bytes[3..]);
    }

    // UTF-16 LE BOM
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_le(&bytes[2..]);
    }

    // UTF-16 BE BOM
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_be(&bytes[2..]);
    }

    // UTF-16 LE without BOM — before UTF-8: ASCII in UTF-16 is valid UTF-8 byte sequence
    if looks_like_utf16_le(bytes) {
        return decode_utf16_le(bytes);
    }

    // Strict UTF-8 (most files)
    if std::str::from_utf8(bytes).is_ok() {
        return Ok(String::from_utf8(bytes.to_vec()).expect("checked"));
    }

    // Last resort — never abort indexing for odd legacy bytes
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn utf8_slice(bytes: &[u8]) -> Result<String, String> {
    std::str::from_utf8(bytes)
        .map(|s| s.to_string())
        .map_err(|e| e.to_string())
}

fn decode_utf16_le(bytes: &[u8]) -> Result<String, String> {
    let even = bytes.len() & !1;
    let units: Vec<u16> = bytes[..even]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).map_err(|e| e.to_string())
}

fn decode_utf16_be(bytes: &[u8]) -> Result<String, String> {
    let even = bytes.len() & !1;
    let units: Vec<u16> = bytes[..even]
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).map_err(|e| e.to_string())
}

fn looks_like_utf16_le(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || bytes.len() % 2 != 0 {
        return false;
    }
    let sample = bytes.len().min(256);
    let mut zero_odd = 0usize;
    let mut pairs = 0usize;
    for i in (1..sample).step_by(2) {
        pairs += 1;
        if bytes[i] == 0 {
            zero_odd += 1;
        }
    }
    pairs > 0 && zero_odd * 3 >= pairs * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_plain() {
        assert_eq!(decode_text_bytes(b"hello").unwrap(), "hello");
    }

    #[test]
    fn utf8_bom() {
        assert_eq!(decode_text_bytes(b"\xEF\xBB\xBFhi").unwrap(), "hi");
    }

    #[test]
    fn utf16_le_bom() {
        let bytes = [0xFF, 0xFE, b'h', 0, b'i', 0];
        assert_eq!(decode_text_bytes(&bytes).unwrap(), "hi");
    }

    #[test]
    fn utf16_le_no_bom() {
        let bytes = [b'A', 0, b'B', 0, b'C', 0];
        assert_eq!(decode_text_bytes(&bytes).unwrap(), "ABC");
    }
}
