use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../relyo-core/src/constants.rs");
    
    // We compute the hash from the live constants at compile time
    let payload = format!(
        "decimals:{} unit:{} supply:{} dust:{} coinbase:{} min_parents:{} max_memo:{} epoch:{} decay:{} years:{} version:{} max_tx:{}",
        relyo_core::constants::RLY_DECIMALS,
        relyo_core::constants::RLY_UNIT,
        relyo_core::constants::TOTAL_SUPPLY,
        relyo_core::constants::DUST_THRESHOLD,
        relyo_core::constants::COINBASE_MATURITY,
        relyo_core::constants::MIN_DAG_PARENTS,
        relyo_core::constants::MAX_MEMO_BYTES,
        relyo_core::constants::EPOCH_DEPTH,
        relyo_core::constants::DECAY_FACTOR_BPS,
        relyo_core::constants::EMISSION_YEARS,
        relyo_core::constants::PROTOCOL_VERSION,
        relyo_core::constants::MAX_TX_SIZE,
    );

    let hash = blake3::hash(payload.as_bytes()).to_hex();

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("consensus_hash.rs");

    fs::write(
        &dest_path,
        format!("pub const CONSENSUS_HASH: &str = \"{}\";\n", hash),
    ).unwrap();
}
