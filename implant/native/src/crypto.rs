//! XOR encryption/decryption with CRC-8 integrity checking.

use anyhow::Result;
use crc8::Crc8;

/// XOR a byte slice with a repeating key
pub fn xor_encrypt(data: &[u8], key: &str) -> Vec<u8> {
    let key_bytes = key.as_bytes();
    let mut result = data.to_vec();
    for (i, byte) in result.iter_mut().enumerate() {
        *byte ^= key_bytes[i % key_bytes.len()];
    }
    result
}

/// Decrypt XOR-encrypted data
pub fn xor_decrypt(data: &[u8], key: &str) -> Vec<u8> {
    // XOR is symmetric: encrypt == decrypt
    xor_encrypt(data, key)
}

/// Compute CRC-8 checksum for integrity verification
pub fn crc8_check(data: &[u8]) -> u8 {
    let mut crc = Crc8::new(&crc8::ALGORITHMS["crc8"]);
    crc.update(data);
    crc.finalize()
}

/// Append CRC-8 checksum to data
pub fn with_crc(data: &[u8]) -> Vec<u8> {
    let mut result = data.to_vec();
    result.push(crc8_check(data));
    result
}

/// Verify CRC-8 checksum (removes the checksum byte if valid)
pub fn verify_crc(data: &[u8]) -> Result<&[u8]> {
    if data.is_empty() {
        return Err(anyhow::anyhow!("Empty data"));
    }
    let (payload, expected_crc) = data.split_at(data.len() - 1);
    let actual_crc = crc8_check(payload);
    if expected_crc[0] == actual_crc {
        Ok(payload)
    } else {
        Err(anyhow::anyhow!(
            "CRC mismatch: expected 0x{:02X}, got 0x{:02X}",
            expected_crc[0],
            actual_crc
        ))
    }
}

/// Encrypt with CRC-8 integrity
pub fn encrypt_with_integrity(data: &[u8], key: &str) -> Vec<u8> {
    let with_crc = with_crc(data);
    xor_encrypt(&with_crc, key)
}

/// Decrypt and verify CRC-8 integrity
pub fn decrypt_with_integrity(data: &[u8], key: &str) -> Result<Vec<u8>> {
    let decrypted = xor_decrypt(data, key);
    let verified = verify_crc(&decrypted)?;
    Ok(verified.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xor_roundtrip() {
        let data = b"Hello, GhostRAT!";
        let key = "GHOSTRATV5";
        let encrypted = xor_encrypt(data, key);
        let decrypted = xor_decrypt(&encrypted, key);
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_crc_roundtrip() {
        let data = b"test data";
        let with_crc = with_crc(data);
        let verified = verify_crc(&with_crc).unwrap();
        assert_eq!(verified, data);
    }

    #[test]
    fn test_crc_detect_corruption() {
        let data = b"test data";
        let mut with_crc = with_crc(data);
        with_crc[0] ^= 0xFF; // corrupt first byte
        assert!(verify_crc(&with_crc).is_err());
    }

    #[test]
    fn test_encrypt_decrypt_with_integrity() {
        let data = b"sensitive payload";
        let key = "KEY123";
        let encrypted = encrypt_with_integrity(data, key);
        let decrypted = decrypt_with_integrity(&encrypted, key).unwrap();
        assert_eq!(decrypted, data);
    }
}
