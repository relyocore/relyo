use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/constants.rs");
    
    let path = Path::new("src/constants.rs");
    let content = fs::read_to_string(path).expect("Failed to read constants.rs");
    
    // Normalize line endings to avoid cross-platform hash mismatch
    let normalized = content.replace("\r\n", "\n");
    let hash = blake3::hash(normalized.as_bytes());
    
    println!("cargo:rustc-env=CONSENSUS_HASH={}", hash.to_hex());
}
