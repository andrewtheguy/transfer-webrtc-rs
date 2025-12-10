use crate::error::{AppError, Result};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;

/// AES-256-GCM key size (32 bytes)
pub const KEY_SIZE: usize = 32;

/// GCM nonce size (12 bytes)
pub const NONCE_SIZE: usize = 12;

/// GCM auth tag size (16 bytes) - included in ciphertext by aes-gcm
pub const TAG_SIZE: usize = 16;

/// Random salt size for nonce generation (4 bytes)
pub const SALT_SIZE: usize = 4;

/// Generate a random 256-bit encryption key
pub fn generate_key() -> [u8; KEY_SIZE] {
    let mut key = [0u8; KEY_SIZE];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

/// Generate a random salt for nonce generation
pub fn generate_salt() -> [u8; SALT_SIZE] {
    let mut salt = [0u8; SALT_SIZE];
    rand::thread_rng().fill_bytes(&mut salt);
    salt
}

/// Create a nonce from chunk index and salt
/// Nonce format: [8 bytes chunk_index (big-endian)] [4 bytes salt]
pub fn create_nonce(chunk_index: u64, salt: &[u8; SALT_SIZE]) -> [u8; NONCE_SIZE] {
    let mut nonce = [0u8; NONCE_SIZE];
    nonce[..8].copy_from_slice(&chunk_index.to_be_bytes());
    nonce[8..].copy_from_slice(salt);
    nonce
}

/// Encrypt a chunk using AES-256-GCM
pub fn encrypt_chunk(
    key: &[u8; KEY_SIZE],
    chunk_index: u64,
    salt: &[u8; SALT_SIZE],
    plaintext: &[u8],
) -> Result<EncryptedChunk> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| AppError::Encryption(format!("Failed to create cipher: {}", e)))?;

    let nonce_bytes = create_nonce(chunk_index, salt);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| AppError::Encryption(format!("Encryption failed: {}", e)))?;

    Ok(EncryptedChunk {
        index: chunk_index,
        nonce: nonce_bytes,
        ciphertext, // includes 16-byte auth tag appended by aes-gcm
    })
}

/// Decrypt a chunk using AES-256-GCM
pub fn decrypt_chunk(key: &[u8; KEY_SIZE], encrypted: &EncryptedChunk) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| AppError::Encryption(format!("Failed to create cipher: {}", e)))?;

    let nonce = Nonce::from_slice(&encrypted.nonce);

    let plaintext = cipher
        .decrypt(nonce, encrypted.ciphertext.as_ref())
        .map_err(|_| AppError::Encryption("Decryption failed: authentication tag mismatch".to_string()))?;

    Ok(plaintext)
}

/// Encode a key as base64 for display
pub fn key_to_base64(key: &[u8; KEY_SIZE]) -> String {
    BASE64.encode(key)
}

/// Decode a base64 key
pub fn key_from_base64(encoded: &str) -> Result<[u8; KEY_SIZE]> {
    let bytes = BASE64
        .decode(encoded.trim())
        .map_err(|e| AppError::Encryption(format!("Invalid base64 key: {}", e)))?;

    if bytes.len() != KEY_SIZE {
        return Err(AppError::Encryption(format!(
            "Invalid key length: expected {} bytes, got {}",
            KEY_SIZE,
            bytes.len()
        )));
    }

    let mut key = [0u8; KEY_SIZE];
    key.copy_from_slice(&bytes);
    Ok(key)
}

/// Encrypted chunk data
#[derive(Debug, Clone)]
pub struct EncryptedChunk {
    pub index: u64,
    pub nonce: [u8; NONCE_SIZE],
    pub ciphertext: Vec<u8>, // includes auth tag
}

impl EncryptedChunk {
    /// Serialize to bytes for transmission
    /// Format: [2 (marker)][8-byte index][12-byte nonce][ciphertext with tag]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(1 + 8 + NONCE_SIZE + self.ciphertext.len());
        bytes.push(2u8); // Message type marker for encrypted chunk
        bytes.extend(&self.index.to_be_bytes());
        bytes.extend(&self.nonce);
        bytes.extend(&self.ciphertext);
        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        // Minimum size: 1 (marker) + 8 (index) + 12 (nonce) + 16 (tag) = 37 bytes
        if data.len() < 37 || data[0] != 2 {
            return None;
        }

        let index = u64::from_be_bytes(data[1..9].try_into().ok()?);
        let nonce: [u8; NONCE_SIZE] = data[9..21].try_into().ok()?;
        let ciphertext = data[21..].to_vec();

        Some(Self {
            index,
            nonce,
            ciphertext,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = generate_key();
        let salt = generate_salt();
        let plaintext = b"Hello, World! This is test data for encryption.";

        let encrypted = encrypt_chunk(&key, 0, &salt, plaintext).unwrap();
        let decrypted = decrypt_chunk(&key, &encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_key_base64_roundtrip() {
        let key = generate_key();
        let encoded = key_to_base64(&key);
        let decoded = key_from_base64(&encoded).unwrap();
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_encrypted_chunk_serialization() {
        let key = generate_key();
        let salt = generate_salt();
        let plaintext = b"Test data";

        let encrypted = encrypt_chunk(&key, 42, &salt, plaintext).unwrap();
        let bytes = encrypted.to_bytes();
        let restored = EncryptedChunk::from_bytes(&bytes).unwrap();

        assert_eq!(encrypted.index, restored.index);
        assert_eq!(encrypted.nonce, restored.nonce);
        assert_eq!(encrypted.ciphertext, restored.ciphertext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = generate_key();
        let key2 = generate_key();
        let salt = generate_salt();
        let plaintext = b"Secret data";

        let encrypted = encrypt_chunk(&key1, 0, &salt, plaintext).unwrap();
        let result = decrypt_chunk(&key2, &encrypted);

        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_data_fails() {
        let key = generate_key();
        let salt = generate_salt();
        let plaintext = b"Secret data";

        let mut encrypted = encrypt_chunk(&key, 0, &salt, plaintext).unwrap();
        // Tamper with ciphertext
        if let Some(byte) = encrypted.ciphertext.get_mut(0) {
            *byte ^= 0xFF;
        }

        let result = decrypt_chunk(&key, &encrypted);
        assert!(result.is_err());
    }
}
