/// NigoriStream serializes types and names into the binary format
/// used by the Nigori protocol for key name derivation (permutation).
///
/// Format: each value is prefixed with its 32-bit big-endian length.
/// For `Type` enum values, the "value" is itself a 4-byte big-endian integer.
pub(crate) struct NigoriStream {
    buf: Vec<u8>,
}

impl NigoriStream {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Append a Type enum value: [4-byte length = 4][4-byte BE type value].
    pub fn push_type(mut self, t: u32) -> Self {
        self.buf.extend_from_slice(&4u32.to_be_bytes());
        self.buf.extend_from_slice(&t.to_be_bytes());
        self
    }

    /// Append a string: [4-byte BE length][string bytes].
    pub fn push_str(mut self, s: &str) -> Self {
        self.buf.extend_from_slice(&(s.len() as u32).to_be_bytes());
        self.buf.extend_from_slice(s.as_bytes());
        self
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    #[test]
    fn nigori_stream_format() {
        let stream = NigoriStream::new().push_type(1).push_str("nigori-key");
        let encoded = STANDARD.encode(stream.into_bytes());
        assert_eq!(encoded, "AAAABAAAAAEAAAAKbmlnb3JpLWtleQ==");
    }
}
