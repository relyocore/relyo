use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use relyo_core::crypto::{sha3_256, KeyPair, SecretKey};
use relyo_core::error::{RelyoError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use zeroize::Zeroize;

/// Encrypted keystore format for persisting wallet keys to disk.
///
/// Uses Argon2id for key derivation and AES-256-GCM for authenticated encryption.
/// This provides:
/// - Memory-hard key derivation resistant to GPU/ASIC brute-force attacks
/// - Authenticated encryption that detects tampering or wrong passphrase
/// - Unique salt and nonce per keystore file
#[derive(Debug, Serialize, Deserialize)]
pub struct KeyStore {
    /// AES-256-GCM encrypted secret key bytes (with auth tag appended).
    pub ciphertext: Vec<u8>,
    /// Argon2id salt for key derivation (32 bytes).
    pub salt: [u8; 32],
    /// AES-256-GCM nonce (12 bytes).
    pub nonce: [u8; 12],
    /// SHA3-256 checksum of the public key for verification after decryption.
    pub pubkey_checksum: [u8; 32],
    /// Version of the keystore format.
    pub version: u8,
}

impl KeyStore {
    /// Encrypt and save a secret key to a file.
    pub fn save(
        secret: &SecretKey,
        passphrase: &str,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let mut salt = [0u8; 32];
        rand::Rng::fill(&mut rand::thread_rng(), &mut salt);

        let mut nonce_bytes = [0u8; 12];
        rand::Rng::fill(&mut rand::thread_rng(), &mut nonce_bytes);

        // Derive a 32-byte key from passphrase using Argon2id
        let mut derived_key = [0u8; 32];
        Argon2::default()
            .hash_password_into(passphrase.as_bytes(), &salt, &mut derived_key)
            .map_err(|e| RelyoError::Wallet(format!("key derivation failed: {}", e)))?;

        // Encrypt with AES-256-GCM (provides both confidentiality and authenticity)
        let cipher = Aes256Gcm::new_from_slice(&derived_key)
            .map_err(|e| RelyoError::Wallet(format!("cipher init failed: {}", e)))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = secret.as_bytes();
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|e| RelyoError::Wallet(format!("encryption failed: {}", e)))?;

        // Zeroize the derived key immediately after use
        derived_key.zeroize();

        // Store checksum of the public key (not the secret!) for post-decryption verification
        let kp = KeyPair::from_secret(secret);
        let pubkey_checksum = sha3_256(kp.public_key.as_bytes());

        let store = KeyStore {
            ciphertext,
            salt,
            nonce: nonce_bytes,
            pubkey_checksum,
            version: 2,
        };

        let json = serde_json::to_string_pretty(&store)?;
        fs::write(path, json).map_err(|e| RelyoError::Wallet(e.to_string()))?;

        Ok(())
    }

    /// Load and decrypt a secret key from a file.
    pub fn load(path: impl AsRef<Path>, passphrase: &str) -> Result<KeyPair> {
        let data = fs::read_to_string(path)
            .map_err(|e| RelyoError::Wallet(e.to_string()))?;

        let store: KeyStore = serde_json::from_str(&data)?;

        // Support both v1 (legacy XOR) and v2 (AES-256-GCM)
        match store.version {
            1 => Self::load_v1_legacy(&store, passphrase),
            2 => Self::load_v2(&store, passphrase),
            _ => Err(RelyoError::Wallet(format!(
                "unsupported keystore version: {}",
                store.version
            ))),
        }
    }

    /// Decrypt a v2 keystore (AES-256-GCM + Argon2id).
    fn load_v2(store: &KeyStore, passphrase: &str) -> Result<KeyPair> {
        // Derive key from passphrase
        let mut derived_key = [0u8; 32];
        Argon2::default()
            .hash_password_into(passphrase.as_bytes(), &store.salt, &mut derived_key)
            .map_err(|e| RelyoError::Wallet(format!("key derivation failed: {}", e)))?;

        // Decrypt with AES-256-GCM
        let cipher = Aes256Gcm::new_from_slice(&derived_key)
            .map_err(|e| RelyoError::Wallet(format!("cipher init failed: {}", e)))?;
        let nonce = Nonce::from_slice(&store.nonce);
        let plaintext = cipher
            .decrypt(nonce, store.ciphertext.as_ref())
            .map_err(|_| RelyoError::Wallet(
                "incorrect passphrase or corrupted keystore".into(),
            ))?;

        // Zeroize derived key
        derived_key.zeroize();

        let secret_bytes: [u8; 32] = plaintext
            .try_into()
            .map_err(|_| RelyoError::Wallet("invalid key length".into()))?;

        let secret = SecretKey::from_bytes(secret_bytes);
        let kp = KeyPair::from_secret(&secret);

        // Verify public key checksum
        let checksum = sha3_256(kp.public_key.as_bytes());
        if checksum != store.pubkey_checksum {
            return Err(RelyoError::Wallet(
                "key integrity check failed — decrypted key does not match expected public key".into(),
            ));
        }

        Ok(kp)
    }

    /// Backward-compatible decryption for v1 keystores (XOR-based).
    /// Users should re-encrypt with v2 format after loading.
    fn load_v1_legacy(store: &KeyStore, passphrase: &str) -> Result<KeyPair> {
        let derived = derive_key_v1(passphrase, &store.salt);

        let decrypted: Vec<u8> = store
            .ciphertext
            .iter()
            .zip(derived.iter().cycle())
            .map(|(e, k)| e ^ k)
            .collect();

        // V1 stored checksum of plaintext secret key in pubkey_checksum field
        let checksum = sha3_256(&decrypted);
        if checksum != store.pubkey_checksum {
            return Err(RelyoError::Wallet(
                "incorrect passphrase or corrupted keystore".into(),
            ));
        }

        let secret_bytes: [u8; 32] = decrypted
            .try_into()
            .map_err(|_| RelyoError::Wallet("invalid key length".into()))?;

        let secret = SecretKey::from_bytes(secret_bytes);
        Ok(KeyPair::from_secret(&secret))
    }

    /// Check if a keystore file exists at the given path.
    pub fn exists(path: impl AsRef<Path>) -> bool {
        path.as_ref().exists()
    }
}

/// Legacy v1 key derivation (kept only for backward compatibility).
fn derive_key_v1(passphrase: &str, salt: &[u8; 32]) -> [u8; 32] {
    let mut input = Vec::with_capacity(passphrase.len() + 32);
    input.extend_from_slice(passphrase.as_bytes());
    input.extend_from_slice(salt);

    let mut key = sha3_256(&input);
    for _ in 0..10_000 {
        key = sha3_256(&key);
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use relyo_core::Address;
    use tempfile::TempDir;

    #[test]
    fn test_keystore_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.key");

        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);

        KeyStore::save(&kp.secret(), "test-password", &path).unwrap();
        assert!(KeyStore::exists(&path));

        let loaded = KeyStore::load(&path, "test-password").unwrap();
        let loaded_addr = Address::from_public_key(&loaded.public_key);
        assert_eq!(addr, loaded_addr);
    }

    #[test]
    fn test_wrong_passphrase() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.key");

        let kp = KeyPair::generate();
        KeyStore::save(&kp.secret(), "correct", &path).unwrap();

        assert!(KeyStore::load(&path, "wrong").is_err());
    }

    #[test]
    fn test_different_wallets_different_ciphertext() {
        let tmp = TempDir::new().unwrap();
        let path1 = tmp.path().join("test1.key");
        let path2 = tmp.path().join("test2.key");

        let kp = KeyPair::generate();
        KeyStore::save(&kp.secret(), "same-password", &path1).unwrap();
        KeyStore::save(&kp.secret(), "same-password", &path2).unwrap();

        let data1 = fs::read_to_string(&path1).unwrap();
        let data2 = fs::read_to_string(&path2).unwrap();
        let store1: KeyStore = serde_json::from_str(&data1).unwrap();
        let store2: KeyStore = serde_json::from_str(&data2).unwrap();

        // Different salt and nonce means different ciphertext even for same key+password
        assert_ne!(store1.salt, store2.salt);
        assert_ne!(store1.ciphertext, store2.ciphertext);
    }
}
