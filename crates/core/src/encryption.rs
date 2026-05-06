//! AES-GCM-256 encryption for at-rest secrets (IMAP passwords).
//!
//! Wire format for stored ciphertext: `nonce(12) || ciphertext || tag(16)`,
//! base64-encoded. The tag is appended to the ciphertext by aes-gcm itself,
//! so we only ever splice the nonce ourselves.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use anyhow::{Context, Result};
use base64::Engine;

#[derive(Clone)]
pub struct Encryptor {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for Encryptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encryptor").finish_non_exhaustive()
    }
}

impl Encryptor {
    pub fn new(key: &[u8; 32]) -> Self {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
        Self { cipher }
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
        let mut combined = Vec::with_capacity(nonce.len() + ciphertext.len());
        combined.extend_from_slice(&nonce);
        combined.extend_from_slice(&ciphertext);
        Ok(base64::engine::general_purpose::STANDARD.encode(combined))
    }

    pub fn decrypt(&self, encoded: &str) -> Result<String> {
        let combined = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .context("ciphertext is not valid base64")?;
        if combined.len() < 12 {
            anyhow::bail!("ciphertext shorter than nonce");
        }
        let (nonce_bytes, ct) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, ct)
            .map_err(|_| anyhow::anyhow!("decryption failed (key mismatch or tampered data)"))?;
        Ok(String::from_utf8(plaintext)
            .context("decrypted bytes are not valid UTF-8")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = [42u8; 32];
        let enc = Encryptor::new(&key);
        let ct = enc.encrypt("hunter2").unwrap();
        assert_eq!(enc.decrypt(&ct).unwrap(), "hunter2");
    }

    #[test]
    fn distinct_ciphertexts_for_same_plaintext() {
        let key = [7u8; 32];
        let enc = Encryptor::new(&key);
        let a = enc.encrypt("same").unwrap();
        let b = enc.encrypt("same").unwrap();
        assert_ne!(a, b, "nonce must randomize output");
    }

    #[test]
    fn wrong_key_fails() {
        let ct = Encryptor::new(&[1u8; 32]).encrypt("x").unwrap();
        assert!(Encryptor::new(&[2u8; 32]).decrypt(&ct).is_err());
    }
}
