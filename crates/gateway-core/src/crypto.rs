use ring::digest;

/// Computes a SHA-256 hash of the input bytes and returns it as a hex string.
pub fn sha256_hex(input: &[u8]) -> String {
    let digest = digest::digest(&digest::SHA256, input);
    hex_encode(digest.as_ref())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_deterministic() {
        let hash1 = sha256_hex(b"test-api-key");
        let hash2 = sha256_hex(b"test-api-key");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_sha256_different_inputs() {
        let hash1 = sha256_hex(b"key-a");
        let hash2 = sha256_hex(b"key-b");
        assert_ne!(hash1, hash2);
    }
}
