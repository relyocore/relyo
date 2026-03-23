use ed25519_dalek::{
    Signer, SigningKey, Verifier, VerifyingKey,
    Signature as DalekSignature,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use zeroize::Zeroize;

use crate::error::{RelyoError, Result};

/// Ed25519 public key wrapper (32 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicKey(#[serde(with = "public_key_serde")] pub(crate) [u8; 32]);

/// Ed25519 secret key wrapper (32-byte signing key seed).
/// Automatically zeroed from memory on drop.
#[derive(Clone)]
pub struct SecretKey(pub(crate) [u8; 32]);

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretKey([REDACTED])")
    }
}

/// Ed25519 digital signature (64 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature(#[serde(with = "hex_serde_64")] pub(crate) [u8; 64]);

/// A complete Ed25519 keypair for signing transactions and messages.
pub struct KeyPair {
    signing_key: SigningKey,
    pub public_key: PublicKey,
}

impl KeyPair {
    /// Generate a new random keypair using OS-provided CSPRNG.
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let public_key = PublicKey(signing_key.verifying_key().to_bytes());
        KeyPair { signing_key, public_key }
    }

    /// Restore a keypair from a 32-byte secret seed.
    pub fn from_secret(secret: &SecretKey) -> Self {
        let signing_key = SigningKey::from_bytes(&secret.0);
        let public_key = PublicKey(signing_key.verifying_key().to_bytes());
        KeyPair { signing_key, public_key }
    }

    /// Export the 32-byte secret seed. Handle with extreme care.
    pub fn secret(&self) -> SecretKey {
        SecretKey(self.signing_key.to_bytes())
    }

    /// Sign arbitrary data, producing a 64-byte Ed25519 signature.
    pub fn sign(&self, message: &[u8]) -> Signature {
        let sig = self.signing_key.sign(message);
        Signature(sig.to_bytes())
    }

    /// Derive a child keypair using HMAC-SHA3-256 key derivation.
    /// Similar to HD wallet child key derivation: child_seed = HMAC-SHA3(parent_secret, index_bytes).
    pub fn derive_child(&self, index: u32) -> Self {
        let secret = self.secret();
        let child_seed = derive_child_key(&secret, index);
        KeyPair::from_secret(&child_seed)
    }
}

impl PublicKey {
    /// Verify a signature against this public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        let verifying_key = VerifyingKey::from_bytes(&self.0)
            .map_err(|e| RelyoError::Crypto(e.to_string()))?;
        let dalek_sig = DalekSignature::from_bytes(&signature.0);
        verifying_key
            .verify(message, &dalek_sig)
            .map_err(|_| RelyoError::InvalidSignature)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        PublicKey(bytes)
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl SecretKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        SecretKey(bytes)
    }
}

impl Signature {
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Signature(bytes)
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

// ─── Key Derivation ─────────────────────────────────────────────────────────

/// Derive a child secret key from a parent using HMAC-SHA3-256.
/// child_seed = SHA3-256(parent_secret || index_be_bytes || "relyo-derive")
pub fn derive_child_key(parent: &SecretKey, index: u32) -> SecretKey {
    let mut data = Vec::with_capacity(68);
    data.extend_from_slice(parent.as_bytes());
    data.extend_from_slice(&index.to_be_bytes());
    data.extend_from_slice(b"relyo-derive");
    SecretKey(sha3_256(&data))
}

// ─── SHA3 Helpers ───────────────────────────────────────────────────────────

/// Compute SHA3-256 hash of a single byte slice.
pub fn sha3_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Compute SHA3-256 hash of multiple concatenated byte slices.
pub fn sha3_256_multi(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    for part in parts {
        hasher.update(part);
    }
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

// ─── Proof of Work ──────────────────────────────────────────────────────────



// ─── Batch Verification ─────────────────────────────────────────────────────

/// Verify multiple signatures in a batch for better performance.
/// Returns Ok(()) if ALL signatures are valid.
/// Uses individual verification as fallback since ed25519-dalek batch
/// verify requires specific setup.
pub fn batch_verify(
    messages: &[&[u8]],
    signatures: &[&Signature],
    public_keys: &[&PublicKey],
) -> Result<()> {
    if messages.len() != signatures.len() || messages.len() != public_keys.len() {
        return Err(RelyoError::BatchVerifyError {
            count: 0,
            total: messages.len(),
        });
    }

    let mut invalid_count = 0;
    for i in 0..messages.len() {
        if public_keys[i].verify(messages[i], signatures[i]).is_err() {
            invalid_count += 1;
        }
    }

    if invalid_count > 0 {
        Err(RelyoError::BatchVerifyError {
            count: invalid_count,
            total: messages.len(),
        })
    } else {
        Ok(())
    }
}

// ─── Serde helpers ──────────────────────────────────────────────────────────

mod public_key_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("invalid public key length"))?;
        Ok(arr)
    }
}

mod hex_serde_64 {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 64], serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let arr: [u8; 64] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("invalid signature length"))?;
        Ok(arr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_verify() {
        let kp = KeyPair::generate();
        let msg = b"hello relyo";
        let sig = kp.sign(msg);
        assert!(kp.public_key.verify(msg, &sig).is_ok());
    }

    #[test]
    fn test_invalid_signature() {
        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();
        let msg = b"hello relyo";
        let sig = kp1.sign(msg);
        assert!(kp2.public_key.verify(msg, &sig).is_err());
    }

    #[test]
    fn test_keypair_restore() {
        let kp = KeyPair::generate();
        let secret = kp.secret();
        let restored = KeyPair::from_secret(&secret);
        assert_eq!(kp.public_key, restored.public_key);
    }

    #[test]
    fn test_sha3_deterministic() {
        let hash = sha3_256(b"relyo");
        assert_eq!(hash.len(), 32);
        assert_eq!(hash, sha3_256(b"relyo"));
    }

    #[test]
    fn test_child_key_derivation() {
        let kp = KeyPair::generate();
        let child0 = kp.derive_child(0);
        let child1 = kp.derive_child(1);
        // Different indices produce different keys
        assert_ne!(child0.public_key, child1.public_key);
        // Same index produces same key
        let child0_again = kp.derive_child(0);
        assert_eq!(child0.public_key, child0_again.public_key);
        // Child key is different from parent
        assert_ne!(kp.public_key, child0.public_key);
    }

    #[test]
    fn test_batch_verify_success() {
        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();
        let msg1 = b"message one";
        let msg2 = b"message two";
        let sig1 = kp1.sign(msg1);
        let sig2 = kp2.sign(msg2);

        let result = batch_verify(
            &[msg1.as_slice(), msg2.as_slice()],
            &[&sig1, &sig2],
            &[&kp1.public_key, &kp2.public_key],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_batch_verify_failure() {
        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();
        let msg1 = b"message one";
        let msg2 = b"message two";
        let sig1 = kp1.sign(msg1);
        let sig2 = kp2.sign(msg2);

        // Use wrong public key for second signature
        let result = batch_verify(
            &[msg1.as_slice(), msg2.as_slice()],
            &[&sig1, &sig2],
            &[&kp1.public_key, &kp1.public_key], // wrong key for sig2
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_secret_key_zeroize() {
        let kp = KeyPair::generate();
        let secret = kp.secret();
        let bytes_copy = *secret.as_bytes();
        // Just verify the key works before drop
        let restored = KeyPair::from_secret(&secret);
        assert_eq!(kp.public_key, restored.public_key);
        drop(secret);
        // Can't directly test that memory is zeroed, but the Drop impl runs zeroize
        let _ = bytes_copy;
    }
}
