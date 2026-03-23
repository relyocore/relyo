use redb::{Database, ReadableTableMetadata, TableDefinition};
use relyo_core::{Address, RelyoError, Result, Transaction, TransactionHash, TransactionStatus};
use std::path::Path;
use tracing::debug;

const TRANSACTIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("transactions");
const STATUS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("status");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
const BALANCES: TableDefinition<&[u8], u64> = TableDefinition::new("balances");
const NONCES: TableDefinition<&[u8], u64> = TableDefinition::new("nonces");
const CHECKPOINTS: TableDefinition<u64, &[u8]> = TableDefinition::new("checkpoints");

/// Persistent storage layer for the DAG backed by redb (pure Rust).
pub struct DagStorage {
    db: Database,
}

impl DagStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db_path = path.as_ref().join("relyo.redb");
        let db = Database::create(&db_path)
            .map_err(|e| RelyoError::Storage(e.to_string()))?;

        // Ensure all tables exist
        let write_txn = db
            .begin_write()
            .map_err(|e| RelyoError::Storage(e.to_string()))?;
        {
            let _ = write_txn.open_table(TRANSACTIONS).map_err(|e| RelyoError::Storage(e.to_string()))?;
            let _ = write_txn.open_table(STATUS).map_err(|e| RelyoError::Storage(e.to_string()))?;
            let _ = write_txn.open_table(META).map_err(|e| RelyoError::Storage(e.to_string()))?;
            let _ = write_txn.open_table(BALANCES).map_err(|e| RelyoError::Storage(e.to_string()))?;
            let _ = write_txn.open_table(NONCES).map_err(|e| RelyoError::Storage(e.to_string()))?;
            let _ = write_txn.open_table(CHECKPOINTS).map_err(|e| RelyoError::Storage(e.to_string()))?;
        }
        write_txn.commit().map_err(|e| RelyoError::Storage(e.to_string()))?;

        debug!("storage opened at {:?}", db_path);
        Ok(DagStorage { db })
    }

    pub fn put_transaction(&self, hash: &TransactionHash, tx: &Transaction) -> Result<()> {
        let data = bincode::serialize(tx)?;
        let write_txn = self.db.begin_write().map_err(|e| RelyoError::Storage(e.to_string()))?;
        {
            let mut table = write_txn.open_table(TRANSACTIONS).map_err(|e| RelyoError::Storage(e.to_string()))?;
            table.insert(hash.as_bytes().as_slice(), data.as_slice()).map_err(|e| RelyoError::Storage(e.to_string()))?;
        }
        write_txn.commit().map_err(|e| RelyoError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_transaction(&self, hash: &TransactionHash) -> Result<Option<Transaction>> {
        let read_txn = self.db.begin_read().map_err(|e| RelyoError::Storage(e.to_string()))?;
        let table = read_txn.open_table(TRANSACTIONS).map_err(|e| RelyoError::Storage(e.to_string()))?;
        match table.get(hash.as_bytes().as_slice()).map_err(|e| RelyoError::Storage(e.to_string()))? {
            Some(data) => {
                let tx: Transaction = bincode::deserialize(data.value())?;
                Ok(Some(tx))
            }
            None => Ok(None),
        }
    }

    pub fn put_status(&self, hash: &TransactionHash, status: TransactionStatus) -> Result<()> {
        let data = bincode::serialize(&status)?;
        let write_txn = self.db.begin_write().map_err(|e| RelyoError::Storage(e.to_string()))?;
        {
            let mut table = write_txn.open_table(STATUS).map_err(|e| RelyoError::Storage(e.to_string()))?;
            table.insert(hash.as_bytes().as_slice(), data.as_slice()).map_err(|e| RelyoError::Storage(e.to_string()))?;
        }
        write_txn.commit().map_err(|e| RelyoError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_status(&self, hash: &TransactionHash) -> Result<Option<TransactionStatus>> {
        let read_txn = self.db.begin_read().map_err(|e| RelyoError::Storage(e.to_string()))?;
        let table = read_txn.open_table(STATUS).map_err(|e| RelyoError::Storage(e.to_string()))?;
        match table.get(hash.as_bytes().as_slice()).map_err(|e| RelyoError::Storage(e.to_string()))? {
            Some(data) => {
                let status: TransactionStatus = bincode::deserialize(data.value())?;
                Ok(Some(status))
            }
            None => Ok(None),
        }
    }

    pub fn put_meta(&self, key: &str, value: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(|e| RelyoError::Storage(e.to_string()))?;
        {
            let mut table = write_txn.open_table(META).map_err(|e| RelyoError::Storage(e.to_string()))?;
            table.insert(key, value).map_err(|e| RelyoError::Storage(e.to_string()))?;
        }
        write_txn.commit().map_err(|e| RelyoError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read().map_err(|e| RelyoError::Storage(e.to_string()))?;
        let table = read_txn.open_table(META).map_err(|e| RelyoError::Storage(e.to_string()))?;
        match table.get(key).map_err(|e| RelyoError::Storage(e.to_string()))? {
            Some(data) => Ok(Some(data.value().to_vec())),
            None => Ok(None),
        }
    }

    pub fn put_balance(&self, addr: &Address, amount: u64) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(|e| RelyoError::Storage(e.to_string()))?;
        {
            let mut table = write_txn.open_table(BALANCES).map_err(|e| RelyoError::Storage(e.to_string()))?;
            table.insert(addr.as_str().as_bytes(), amount).map_err(|e| RelyoError::Storage(e.to_string()))?;
        }
        write_txn.commit().map_err(|e| RelyoError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_balance(&self, addr: &Address) -> Result<u64> {
        let read_txn = self.db.begin_read().map_err(|e| RelyoError::Storage(e.to_string()))?;
        let table = read_txn.open_table(BALANCES).map_err(|e| RelyoError::Storage(e.to_string()))?;
        match table.get(addr.as_str().as_bytes()).map_err(|e| RelyoError::Storage(e.to_string()))? {
            Some(data) => Ok(data.value()),
            None => Ok(0),
        }
    }

    pub fn put_nonce(&self, addr: &Address, nonce: u64) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(|e| RelyoError::Storage(e.to_string()))?;
        {
            let mut table = write_txn.open_table(NONCES).map_err(|e| RelyoError::Storage(e.to_string()))?;
            table.insert(addr.as_str().as_bytes(), nonce).map_err(|e| RelyoError::Storage(e.to_string()))?;
        }
        write_txn.commit().map_err(|e| RelyoError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_nonce(&self, addr: &Address) -> Result<u64> {
        let read_txn = self.db.begin_read().map_err(|e| RelyoError::Storage(e.to_string()))?;
        let table = read_txn.open_table(NONCES).map_err(|e| RelyoError::Storage(e.to_string()))?;
        match table.get(addr.as_str().as_bytes()).map_err(|e| RelyoError::Storage(e.to_string()))? {
            Some(data) => Ok(data.value()),
            None => Ok(0),
        }
    }

    pub fn put_checkpoint(&self, epoch: u64, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(|e| RelyoError::Storage(e.to_string()))?;
        {
            let mut table = write_txn.open_table(CHECKPOINTS).map_err(|e| RelyoError::Storage(e.to_string()))?;
            table.insert(epoch, data).map_err(|e| RelyoError::Storage(e.to_string()))?;
        }
        write_txn.commit().map_err(|e| RelyoError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_checkpoint(&self, epoch: u64) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read().map_err(|e| RelyoError::Storage(e.to_string()))?;
        let table = read_txn.open_table(CHECKPOINTS).map_err(|e| RelyoError::Storage(e.to_string()))?;
        match table.get(epoch).map_err(|e| RelyoError::Storage(e.to_string()))? {
            Some(data) => Ok(Some(data.value().to_vec())),
            None => Ok(None),
        }
    }

    pub fn exists(&self, hash: &TransactionHash) -> Result<bool> {
        Ok(self.get_transaction(hash)?.is_some())
    }

    pub fn transaction_count(&self) -> Result<usize> {
        let read_txn = self.db.begin_read().map_err(|e| RelyoError::Storage(e.to_string()))?;
        let table = read_txn.open_table(TRANSACTIONS).map_err(|e| RelyoError::Storage(e.to_string()))?;
        Ok(table.len().map_err(|e: redb::StorageError| RelyoError::Storage(e.to_string()))? as usize)
    }

    pub fn put_transaction_batch(&self, txs: &[(TransactionHash, Transaction)]) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(|e| RelyoError::Storage(e.to_string()))?;
        {
            let mut table = write_txn.open_table(TRANSACTIONS).map_err(|e| RelyoError::Storage(e.to_string()))?;
            for (hash, tx) in txs {
                let data = bincode::serialize(tx)?;
                table.insert(hash.as_bytes().as_slice(), data.as_slice()).map_err(|e| RelyoError::Storage(e.to_string()))?;
            }
        }
        write_txn.commit().map_err(|e| RelyoError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relyo_core::{crypto::KeyPair, token::RELYO_CONFIG, transaction::TransactionBuilder, Address};
    use tempfile::TempDir;

    #[test]
    fn test_storage_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let storage = DagStorage::open(tmp.path()).unwrap();

        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        let recv = Address::from_public_key(&KeyPair::generate().public_key);
        let tx = TransactionBuilder::new(addr, recv, 100, RELYO_CONFIG.base_fee, 1).sign(&kp);
        let hash = tx.hash();

        storage.put_transaction(&hash, &tx).unwrap();
        let loaded = storage.get_transaction(&hash).unwrap().unwrap();
        assert_eq!(loaded.hash(), hash);

        storage.put_status(&hash, TransactionStatus::Confirmed).unwrap();
        let status = storage.get_status(&hash).unwrap().unwrap();
        assert_eq!(status, TransactionStatus::Confirmed);
    }

    #[test]
    fn test_meta() {
        let tmp = TempDir::new().unwrap();
        let storage = DagStorage::open(tmp.path()).unwrap();
        storage.put_meta("height", b"42").unwrap();
        let val = storage.get_meta("height").unwrap().unwrap();
        assert_eq!(val, b"42");
    }

    #[test]
    fn test_balance_nonce() {
        let tmp = TempDir::new().unwrap();
        let storage = DagStorage::open(tmp.path()).unwrap();
        let kp = KeyPair::generate();
        let addr = Address::from_public_key(&kp.public_key);
        storage.put_balance(&addr, 1_000_000).unwrap();
        assert_eq!(storage.get_balance(&addr).unwrap(), 1_000_000);
        storage.put_nonce(&addr, 5).unwrap();
        assert_eq!(storage.get_nonce(&addr).unwrap(), 5);
    }

    #[test]
    fn test_checkpoint() {
        let tmp = TempDir::new().unwrap();
        let storage = DagStorage::open(tmp.path()).unwrap();
        storage.put_checkpoint(1, b"checkpoint data").unwrap();
        let loaded = storage.get_checkpoint(1).unwrap().unwrap();
        assert_eq!(loaded, b"checkpoint data");
        assert!(storage.get_checkpoint(2).unwrap().is_none());
    }
}
