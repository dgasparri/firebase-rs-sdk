use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BytesValue(Vec<u8>);

impl BytesValue {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn from_base64(value: &str) -> Result<Self, base64::DecodeError> {
        STANDARD_NO_PAD.decode(value).map(Self)
    }

    pub fn to_base64(&self) -> String {
        STANDARD_NO_PAD.encode(&self.0)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for BytesValue {
    fn from(value: Vec<u8>) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        let bytes = BytesValue::new(vec![1, 2, 3, 4]);
        let encoded = bytes.to_base64();
        let decoded = BytesValue::from_base64(&encoded).unwrap();
        assert_eq!(decoded.as_slice(), &[1, 2, 3, 4]);
    }
}
