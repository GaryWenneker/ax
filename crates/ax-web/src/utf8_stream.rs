//! Decode a byte stream that arrives in chunks as UTF-8 text.

//!
//! A chunk boundary can fall inside a multi-byte character. Decoding each
//! chunk on its own would turn both halves into U+FFFD; these helpers hold
//! the incomplete end back until the rest arrives.

/// Turns chunks of a byte stream into text. Bytes that start a character
/// but end the chunk are kept for the next call; bytes that can never form a
/// character become U+FFFD.
#[derive(Default)]
pub(crate) struct Utf8Decoder {
    carry: Vec<u8>,
}

impl Utf8Decoder {
    /// The text for `chunk`, joined with what the previous chunk left over.
    pub(crate) fn decode(&mut self, chunk: &[u8]) -> String {
        self.carry.extend_from_slice(chunk);
        let (text, used) = decode_complete(&self.carry);
        self.carry.drain(..used);
        text
    }
}

/// How many leading bytes of `bytes` form whole characters (or invalid
/// sequences); the rest is the start of a character still being written.
pub(crate) fn complete_len(bytes: &[u8]) -> usize {
    decode_complete(bytes).1
}

/// Decode the leading bytes of `bytes` that do not end in an incomplete
/// character, and say how many bytes that was.
fn decode_complete(bytes: &[u8]) -> (String, usize) {
    let mut out = String::with_capacity(bytes.len());
    let mut used = 0;
    loop {
        match std::str::from_utf8(&bytes[used..]) {
            Ok(text) => {
                out.push_str(text);
                return (out, bytes.len());
            }
            Err(error) => {
                let valid = used + error.valid_up_to();
                out.push_str(std::str::from_utf8(&bytes[used..valid]).unwrap_or_default());
                match error.error_len() {
                    Some(bad) => {
                        out.push('\u{fffd}');
                        used = valid + bad;
                    }
                    None => return (out, valid),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "┌─ box ─┐ 🦀 naïve €";

    #[test]
    fn a_character_split_across_chunks_comes_out_whole() {
        let bytes = TEXT.as_bytes();
        for cut in 1..bytes.len() {
            let mut decoder = Utf8Decoder::default();
            let mut out = decoder.decode(&bytes[..cut]);
            out.push_str(&decoder.decode(&bytes[cut..]));
            assert_eq!(out, TEXT, "cut at {cut}");
        }
    }

    #[test]
    fn one_byte_at_a_time_still_decodes() {
        let mut decoder = Utf8Decoder::default();
        let out: String = TEXT
            .as_bytes()
            .iter()
            .map(|b| decoder.decode(&[*b]))
            .collect();
        assert_eq!(out, TEXT);
    }

    #[test]
    fn invalid_bytes_become_replacement_characters_and_decoding_goes_on() {
        let mut decoder = Utf8Decoder::default();
        let mut out = decoder.decode(b"a\xffb");
        out.push_str(&decoder.decode("─".as_bytes()));
        assert_eq!(out, "a\u{fffd}b─");
        // A lead byte followed by a byte that cannot continue it.
        let mut out = decoder.decode(b"\xe2");
        out.push_str(&decoder.decode(b"x"));
        assert_eq!(out, "\u{fffd}x");
    }
}
