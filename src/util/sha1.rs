use sha1::{Digest, Sha1};

pub fn sha1_digest(data: impl AsRef<[u8]>) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(data.as_ref());
    let result = hasher.finalize();
    let mut buf = [0u8; 20];
    buf.copy_from_slice(&result);
    buf
}

pub fn sha1_hex(data: impl AsRef<[u8]>) -> String {
    let digest = sha1_digest(data);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_hash_matches() {
        let digest = sha1_hex("The quick brown fox jumps over the lazy dog");
        assert_eq!(digest, "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12");
    }
}
