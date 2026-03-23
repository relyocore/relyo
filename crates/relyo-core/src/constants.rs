//! Consensus critical constants.

pub const RLY_DECIMALS: u8 = 8;
pub const RLY_UNIT: u64 = 100_000_000;
pub const TOTAL_SUPPLY: u64 = 25_000_000_000 * RLY_UNIT;
pub const DUST_THRESHOLD: u64 = 1_000; // 0.00001 RLY
pub const COINBASE_MATURITY: u64 = 100;
pub const MIN_DAG_PARENTS: usize = 2;
pub const MAX_MEMO_BYTES: usize = 256;
pub const EPOCH_DEPTH: u64 = 10_000;
pub const DECAY_FACTOR_BPS: u64 = 9990;
pub const EMISSION_YEARS: u32 = 256;
pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_TX_SIZE: usize = 100_000; // 100 KB
pub const CHAIN_ID: u32 = 1;

pub const R0: f64 = 187_500.0;
pub const LAMBDA: f64 = 0.0000075;

// Compile time assertion that R0 / LAMBDA == 25_000_000_000
const _: () = assert!((R0 / LAMBDA) == 25_000_000_000.0);
