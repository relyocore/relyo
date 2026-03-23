use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::crypto::{sha3_256, PublicKey};
use crate::error::{RelyoError, Result};

/// Network prefix byte for Relyo addresses (0x52 = 'R' in ASCII).
const ADDRESS_PREFIX: u8 = 0x52;

/// Type of Relyo address, encoded in the second byte of the raw address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AddressType {
    /// Standard user address for sending/receiving RLY.
    Standard = 0x01,
    /// Smart contract address (future use).
    Contract = 0x02,
    /// Staking/validator address.
    Staking = 0x03,
    /// Treasury address.
    Treasury = 0x04,
}

impl AddressType {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(AddressType::Standard),
            0x02 => Some(AddressType::Contract),
            0x03 => Some(AddressType::Staking),
            0x04 => Some(AddressType::Treasury),
            _ => None,
        }
    }
}

/// A Relyo network address derived from an Ed25519 public key.
///
/// Raw format (26 bytes):
///   PREFIX (0x52) || TYPE (1 byte) || SHA3-256(pubkey)[0..20] || checksum[0..4]
///
/// Encoded as Base58Check for human readability.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(String);

impl Address {
    /// Derive a standard address from a public key.
    pub fn from_public_key(pubkey: &PublicKey) -> Self {
        Self::from_public_key_typed(pubkey, AddressType::Standard)
    }

    /// Derive an address of specific type from a public key.
    pub fn from_public_key_typed(pubkey: &PublicKey, addr_type: AddressType) -> Self {
        let hash = sha3_256(pubkey.as_bytes());
        let mut payload = Vec::with_capacity(26);
        payload.push(ADDRESS_PREFIX);
        payload.push(addr_type as u8);
        payload.extend_from_slice(&hash[..20]);

        let checksum = sha3_256(&payload);
        payload.extend_from_slice(&checksum[..4]);

        Address(bs58::encode(&payload).into_string())
    }

    /// Construct an address from raw bytes (26 bytes).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 26 {
            return Err(RelyoError::InvalidAddress(format!(
                "expected 26 bytes, got {}",
                bytes.len()
            )));
        }
        if bytes[0] != ADDRESS_PREFIX {
            return Err(RelyoError::InvalidAddress("invalid network prefix".into()));
        }
        // Verify checksum
        let checksum = sha3_256(&bytes[..22]);
        if bytes[22..26] != checksum[..4] {
            return Err(RelyoError::InvalidAddress("checksum mismatch".into()));
        }
        Ok(Address(bs58::encode(bytes).into_string()))
    }

    /// Get the raw bytes of this address.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        bs58::decode(&self.0)
            .into_vec()
            .map_err(|e| RelyoError::InvalidAddress(e.to_string()))
    }

    /// Get the address type.
    pub fn address_type(&self) -> Result<AddressType> {
        let bytes = self.to_bytes()?;
        AddressType::from_byte(bytes[1])
            .ok_or_else(|| RelyoError::InvalidAddress("unknown address type".into()))
    }

    /// Validate that an address string is well-formed.
    pub fn validate(addr: &str) -> Result<()> {
        let bytes = bs58::decode(addr)
            .into_vec()
            .map_err(|e| RelyoError::InvalidAddress(e.to_string()))?;

        if bytes.len() != 26 {
            return Err(RelyoError::InvalidAddress(format!(
                "expected 26 bytes, got {}",
                bytes.len()
            )));
        }

        if bytes[0] != ADDRESS_PREFIX {
            return Err(RelyoError::InvalidAddress("invalid network prefix".into()));
        }

        if AddressType::from_byte(bytes[1]).is_none() {
            return Err(RelyoError::InvalidAddress("unknown address type byte".into()));
        }

        let checksum = sha3_256(&bytes[..22]);
        if bytes[22..26] != checksum[..4] {
            return Err(RelyoError::InvalidAddress("checksum mismatch".into()));
        }

        Ok(())
    }

    /// Check if the checksum is valid without full validation.
    pub fn is_valid_checksum(&self) -> bool {
        if let Ok(bytes) = self.to_bytes() {
            if bytes.len() == 26 {
                let checksum = sha3_256(&bytes[..22]);
                return bytes[22..26] == checksum[..4];
            }
        }
        false
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The genesis address (all-zero hash, Standard type).
    pub fn genesis() -> Self {
        let mut payload = Vec::with_capacity(26);
        payload.push(ADDRESS_PREFIX);
        payload.push(AddressType::Standard as u8);
        payload.extend_from_slice(&[0u8; 20]);
        let checksum = sha3_256(&payload);
        payload.extend_from_slice(&checksum[..4]);
        Address(bs58::encode(&payload).into_string())
    }

    /// Create a treasury address from a public key.
    pub fn treasury(pubkey: &PublicKey) -> Self {
        Self::from_public_key_typed(pubkey, AddressType::Treasury)
    }

    /// Create a staking address from a public key.
    pub fn staking(pubkey: &PublicKey) -> Self {
        Self::from_public_key_typed(pubkey, AddressType::Staking)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Address {
    type Err = RelyoError;

    fn from_str(s: &str) -> Result<Self> {
        Address::validate(s)?;
        Ok(Address(s.to_string()))
    }
}

impl AsRef<str> for Address {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyPair;

    #[test]
    fn test_address_from_pubkey() {
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        assert!(Address::validate(addr.as_str()).is_ok());
    }

    #[test]
    fn test_address_deterministic() {
        let kp = KeyPair::generate();
        let a1 = Address::from_public_key(&kp.public_key);
        let a2 = Address::from_public_key(&kp.public_key);
        assert_eq!(a1, a2);
    }

    #[test]
    fn test_invalid_address() {
        assert!(Address::validate("invalid").is_err());
    }

    #[test]
    fn test_genesis_address() {
        let g = Address::genesis();
        assert!(Address::validate(g.as_str()).is_ok());
    }

    #[test]
    fn test_address_types() {
        let kp = KeyPair::generate();
        let standard = Address::from_public_key(&kp.public_key);
        let staking = Address::staking(&kp.public_key);
        let treasury = Address::treasury(&kp.public_key);

        assert_eq!(standard.address_type().unwrap(), AddressType::Standard);
        assert_eq!(staking.address_type().unwrap(), AddressType::Staking);
        assert_eq!(treasury.address_type().unwrap(), AddressType::Treasury);

        // Different types produce different addresses
        assert_ne!(standard, staking);
        assert_ne!(standard, treasury);
        assert_ne!(staking, treasury);
    }

    #[test]
    fn test_address_bytes_roundtrip() {
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        let bytes = addr.to_bytes().unwrap();
        let addr2 = Address::from_bytes(&bytes).unwrap();
        assert_eq!(addr, addr2);
    }

    #[test]
    fn test_address_checksum() {
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        assert!(addr.is_valid_checksum());
    }
}
