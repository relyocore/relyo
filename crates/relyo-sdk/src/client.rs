use relyo_core::{
    crypto::KeyPair,
    token::RELYO_CONFIG,
    transaction::{TransactionBuilder, TransactionHash},
    Address, Result, Transaction,
};

/// High-level client for interacting with the Relyo network.
///
/// This client provides a simplified interface for common operations:
/// creating wallets, building transactions, and querying state.
pub struct RelyoClient {
    keypair: Option<KeyPair>,
    address: Option<Address>,
}

impl RelyoClient {
    /// Create a new client without a keypair (read-only).
    pub fn new() -> Self {
        RelyoClient {
            keypair: None,
            address: None,
        }
    }

    /// Create a client with a new keypair.
    pub fn with_new_wallet() -> Self {
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        RelyoClient {
            keypair: Some(kp),
            address: Some(addr),
        }
    }

    /// Create a client from an existing secret key.
    pub fn with_secret(secret: &relyo_core::SecretKey) -> Self {
        let kp = KeyPair::from_secret(secret);
        let addr = Address::from_public_key(&kp.public_key);
        RelyoClient {
            keypair: Some(kp),
            address: Some(addr),
        }
    }

    /// Get the client's address, if a keypair is loaded.
    pub fn address(&self) -> Option<&Address> {
        self.address.as_ref()
    }

    /// Get the client's public key hex, if a keypair is loaded.
    pub fn public_key_hex(&self) -> Option<String> {
        self.keypair
            .as_ref()
            .map(|kp| hex::encode(kp.public_key.as_bytes()))
    }

    /// Build and sign a transfer transaction.
    pub fn transfer(
        &self,
        to: Address,
        amount: u64,
        nonce: u64,
        parent_1: TransactionHash,
        parent_2: TransactionHash,
    ) -> Result<Transaction> {
        let kp = self
            .keypair
            .as_ref()
            .ok_or_else(|| relyo_core::RelyoError::Wallet("no keypair loaded".into()))?;

        let addr = self.address.as_ref().ok_or_else(|| relyo_core::RelyoError::Wallet("no address derived".into()))?;

        let tx = TransactionBuilder::new(
            addr.clone(),
            to,
            amount,
            RELYO_CONFIG.base_fee,
            nonce,
        )
        .parents(parent_1, parent_2)
        .sign(kp);

        Ok(tx)
    }

    /// Build a transfer using human-readable RLY amounts.
    pub fn transfer_rly(
        &self,
        to: Address,
        amount_rly: f64,
        nonce: u64,
        parent_1: TransactionHash,
        parent_2: TransactionHash,
    ) -> Result<Transaction> {
        let amount_base = relyo_core::token::rly_to_base(amount_rly);
        self.transfer(to, amount_base, nonce, parent_1, parent_2)
    }

    /// Validate a Relyo address string.
    pub fn validate_address(addr: &str) -> Result<Address> {
        addr.parse()
    }

    /// Derive an address from a public key.
    pub fn address_from_pubkey(pubkey: &relyo_core::PublicKey) -> Address {
        Address::from_public_key(pubkey)
    }

    /// Verify a transaction's signature and sender.
    pub fn verify_transaction(tx: &Transaction) -> Result<()> {
        tx.validate()
    }
}

impl Default for RelyoClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_new_wallet() {
        let client = RelyoClient::with_new_wallet();
        assert!(client.address().is_some());
        assert!(client.public_key_hex().is_some());
    }

    #[test]
    fn test_client_transfer() {
        let client = RelyoClient::with_new_wallet();
        let receiver = RelyoClient::with_new_wallet();

        let tx = client
            .transfer(
                receiver.address().unwrap().clone(),
                1_000_000,
                1,
                TransactionHash::zero(),
                TransactionHash::zero(),
            )
            .unwrap();

        assert!(RelyoClient::verify_transaction(&tx).is_ok());
    }

    #[test]
    fn test_readonly_client() {
        let client = RelyoClient::new();
        assert!(client.address().is_none());

        let addr = RelyoClient::validate_address(
            Address::from_public_key(&KeyPair::generate().public_key).as_ref(),
        );
        assert!(addr.is_ok());
    }
}
