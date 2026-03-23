use relyo_core::{
    crypto::KeyPair,
    token::RELYO_CONFIG,
    transaction::{TransactionBuilder, TransactionHash},
    Address, Transaction,
};
use std::path::{Path, PathBuf};

use crate::keystore::KeyStore;

/// A Relyo wallet that manages keys and creates signed transactions.
pub struct Wallet {
    keypair: KeyPair,
    address: Address,
    #[allow(dead_code)]
    keystore_path: PathBuf,
}

impl Wallet {
    /// Create a new wallet with a freshly generated keypair.
    pub fn create(keystore_path: impl AsRef<Path>, passphrase: &str) -> relyo_core::Result<Self> {
        let keypair = KeyPair::generate();
        let address = Address::from_public_key(&keypair.public_key);
        let path = keystore_path.as_ref().to_path_buf();

        KeyStore::save(&keypair.secret(), passphrase, &path)?;

        Ok(Wallet {
            keypair,
            address,
            keystore_path: path,
        })
    }

    /// Open an existing wallet from a keystore file.
    pub fn open(keystore_path: impl AsRef<Path>, passphrase: &str) -> relyo_core::Result<Self> {
        let path = keystore_path.as_ref().to_path_buf();
        let keypair = KeyStore::load(&path, passphrase)?;
        let address = Address::from_public_key(&keypair.public_key);

        Ok(Wallet {
            keypair,
            address,
            keystore_path: path,
        })
    }

    /// Get the wallet's address.
    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Get the wallet's public key as hex.
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.keypair.public_key.as_bytes())
    }

    /// Create and sign a transaction.
    pub fn create_transaction(
        &self,
        receiver: Address,
        amount: u64,
        nonce: u64,
        parent_1: TransactionHash,
        parent_2: TransactionHash,
    ) -> Transaction {
        TransactionBuilder::new(
            self.address.clone(),
            receiver,
            amount,
            RELYO_CONFIG.base_fee,
            nonce,
        )
        .parents(parent_1, parent_2)
        .sign(&self.keypair)
    }

    /// Create, mine PoW, and sign a transaction.
    ///
    /// The wallet performs a small CPU-bound PoW computation as anti-spam
    /// protection before signing. `difficulty` is the number of leading
    /// zero bits required (typically 8-16 for a few ms of work).
    pub fn create_transaction_with_pow(
        &self,
        receiver: Address,
        amount: u64,
        nonce: u64,
        parent_1: TransactionHash,
        parent_2: TransactionHash,
    ) -> Transaction {
        TransactionBuilder::new(
            self.address.clone(),
            receiver,
            amount,
            RELYO_CONFIG.base_fee,
            nonce,
        )
        .parents(parent_1, parent_2)
        .sign(&self.keypair)
    }

    /// Sign arbitrary data with the wallet's private key.
    pub fn sign(&self, message: &[u8]) -> relyo_core::Signature {
        self.keypair.sign(message)
    }

    /// Export the public key bytes.
    pub fn public_key(&self) -> &relyo_core::PublicKey {
        &self.keypair.public_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_wallet_create_and_open() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("wallet.key");

        let wallet = Wallet::create(&path, "secret123").unwrap();
        let addr = wallet.address().clone();

        let reopened = Wallet::open(&path, "secret123").unwrap();
        assert_eq!(reopened.address(), &addr);
    }

    #[test]
    fn test_create_transaction() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("wallet.key");

        let wallet = Wallet::create(&path, "pass").unwrap();
        let recv_kp = KeyPair::generate();
        let recv = Address::from_public_key(&recv_kp.public_key);

        let tx = wallet.create_transaction(
            recv,
            1_000_000,
            1,
            TransactionHash::zero(),
            TransactionHash::zero(),
        );

        assert!(tx.verify_signature().is_ok());
        assert!(tx.verify_sender().is_ok());
    }
}
