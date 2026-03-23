use relyo_core::{
    crypto::{sha3_256, KeyPair},
    token::{rly_to_base, RELYO_CONFIG},
    Address, Result, Transaction, TransactionHash,
};
use serde::{Deserialize, Serialize};

/// Merchant payment integration API.
///
/// Provides helpers for merchants to:
/// - Generate payment requests with unique reference IDs
/// - Verify incoming payments
/// - Create refund transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequest {
    /// Merchant's receiving address.
    pub merchant_address: String,
    /// Amount in base units.
    pub amount: u64,
    /// Amount in human-readable RLY.
    pub amount_rly: f64,
    /// Unique reference ID for this payment.
    pub reference_id: String,
    /// Optional memo / description.
    pub memo: String,
    /// Expiry timestamp (ms since epoch).
    pub expires_at: u64,
}

/// Merchant payment API.
pub struct MerchantApi {
    keypair: KeyPair,
    address: Address,
}

impl MerchantApi {
    /// Create a new merchant API with a keypair.
    pub fn new(keypair: KeyPair) -> Self {
        let address = Address::from_public_key(&keypair.public_key);
        MerchantApi { keypair, address }
    }

    /// Get the merchant's receiving address.
    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Create a payment request.
    pub fn create_payment_request(
        &self,
        amount_rly: f64,
        memo: &str,
        ttl_seconds: u64,
    ) -> PaymentRequest {
        let amount = rly_to_base(amount_rly);
        let now = relyo_core::now_ms();
        let expires_at = now + (ttl_seconds * 1000);

        // Generate a unique reference ID from the payment details.
        let ref_data = format!("{}{}{}{}", self.address, amount, now, memo);
        let ref_hash = sha3_256(ref_data.as_bytes());
        let reference_id = hex::encode(&ref_hash[..16]); // 16 bytes = 32 hex chars

        PaymentRequest {
            merchant_address: self.address.to_string(),
            amount,
            amount_rly,
            reference_id,
            memo: memo.to_string(),
            expires_at,
        }
    }

    /// Verify that a transaction matches a payment request.
    pub fn verify_payment(
        &self,
        tx: &Transaction,
        request: &PaymentRequest,
    ) -> Result<bool> {
        // Check receiver matches merchant.
        if tx.receiver.to_string() != request.merchant_address {
            return Ok(false);
        }

        // Check amount (must be at least the requested amount).
        if tx.amount < request.amount {
            return Ok(false);
        }

        // Check not expired.
        if tx.timestamp > request.expires_at {
            return Ok(false);
        }

        // Verify transaction signature.
        tx.validate()?;

        Ok(true)
    }

    /// Create a refund transaction.
    pub fn create_refund(
        &self,
        original_tx: &Transaction,
        nonce: u64,
        parent_1: TransactionHash,
        parent_2: TransactionHash,
    ) -> Transaction {
        use relyo_core::transaction::TransactionBuilder;

        TransactionBuilder::new(
            self.address.clone(),
            original_tx.sender.clone(),
            original_tx.amount,
            RELYO_CONFIG.base_fee,
            nonce,
        )
        .parents(parent_1, parent_2)
        .sign(&self.keypair)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payment_request() {
        let kp = KeyPair::generate();
        let api = MerchantApi::new(kp);

        let req = api.create_payment_request(10.0, "Order #123", 3600);
        assert_eq!(req.amount, rly_to_base(10.0));
        assert!(!req.reference_id.is_empty());
    }
}
