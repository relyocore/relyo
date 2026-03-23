use clap::{Parser, Subcommand};
use relyo_core::token::{format_rly, rly_to_base};
use relyo_core::transaction::TransactionHash;
use relyo_core::Address;
use relyo_wallet::{KeyStore, Wallet};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

/// Default PoW difficulty for anti-spam (number of leading zero bits).
/// 12 bits ~ 4096 hash attempts ~ a few milliseconds on modern CPUs.

#[derive(Parser)]
#[command(name = "relyo-wallet")]
#[command(about = "Relyo (RLY) wallet — manage keys and send transactions")]
#[command(version)]
struct Cli {
    /// Path to the keystore file.
    #[arg(short, long, default_value = "wallet.key")]
    keystore: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new wallet.
    Create {
        /// Passphrase to encrypt the keystore.
        #[arg(short, long)]
        passphrase: String,
    },
    /// Show the wallet address and public key.
    Info {
        #[arg(short, long)]
        passphrase: String,
    },
    /// Create and display a signed transaction (offline).
    Sign {
        #[arg(short, long)]
        passphrase: String,
        /// Receiver address.
        #[arg(short, long)]
        to: String,
        /// Amount in RLY (e.g., 1.5).
        #[arg(short, long)]
        amount: f64,
        /// Transaction nonce.
        #[arg(short, long)]
        nonce: u64,
    },
    /// Send a transaction to a running node via JSON-RPC.
    Send {
        #[arg(short, long)]
        passphrase: String,
        /// Receiver address.
        #[arg(short, long)]
        to: String,
        /// Amount in RLY (e.g., 1.5).
        #[arg(short, long)]
        amount: f64,
        /// Node RPC endpoint URL.
        #[arg(short, long, default_value = "http://127.0.0.1:9090")]
        rpc: String,
    },
    /// Query the balance of an address from a running node.
    Balance {
        /// Address to query (defaults to this wallet's address).
        address: Option<String>,
        /// Node RPC endpoint URL.
        #[arg(short, long, default_value = "http://127.0.0.1:9090")]
        rpc: String,
        /// Passphrase (needed if querying own balance without specifying address).
        #[arg(short, long)]
        passphrase: Option<String>,
    },
    /// Validate an address.
    Validate {
        /// Address to validate.
        address: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Create { passphrase } => {
            if KeyStore::exists(&cli.keystore) {
                eprintln!("Error: keystore already exists at {:?}", cli.keystore);
                eprintln!("Delete it first or use a different path.");
                std::process::exit(1);
            }

            let wallet = Wallet::create(&cli.keystore, &passphrase)?;
            println!("Wallet created successfully!");
            println!("Address:    {}", wallet.address());
            println!("Public Key: {}", wallet.public_key_hex());
            println!("Keystore:   {:?}", cli.keystore);
            println!();
            println!("IMPORTANT: Back up your keystore file and remember your passphrase.");
            println!("           If you lose either, your funds will be unrecoverable.");
        }

        Commands::Info { passphrase } => {
            let wallet = Wallet::open(&cli.keystore, &passphrase)?;
            println!("Address:    {}", wallet.address());
            println!("Public Key: {}", wallet.public_key_hex());
        }

        Commands::Sign {
            passphrase,
            to,
            amount,
            nonce,
        } => {
            let wallet = Wallet::open(&cli.keystore, &passphrase)?;
            let receiver: Address = to.parse()?;
            let amount_base = rly_to_base(amount);

            let tx = wallet.create_transaction_with_pow(
                receiver,
                amount_base,
                nonce,
                TransactionHash::zero(),
                TransactionHash::zero(),
            );

            let json = serde_json::to_string_pretty(&tx)?;
            println!("Signed Transaction:");
            println!("{}", json);
            println!();
            println!("Transaction Hash: {}", tx.hash());
        }

        Commands::Send {
            passphrase,
            to,
            amount,
            rpc,
        } => {
            let wallet = Wallet::open(&cli.keystore, &passphrase)?;
            let receiver: Address = to.parse()?;
            let amount_base = rly_to_base(amount);

            println!("Querying nonce and tips from node...");

            let client = reqwest::Client::new();

            // Get sender nonce from RPC.
            let nonce_resp = rpc_call(
                &client,
                &rpc,
                "rly_getNonce",
                serde_json::json!([wallet.address().to_string()]),
            )
            .await?;
            let current_nonce = nonce_resp["nonce"].as_u64().unwrap_or(0);
            let next_nonce = current_nonce + 1;

            // Get DAG tips for parent selection.
            let tips_resp =
                rpc_call(&client, &rpc, "rly_getTips", serde_json::json!([])).await?;
            let tips = tips_resp["tips"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let parent_1 = if !tips.is_empty() {
                TransactionHash::from_hex(&tips[0])?
            } else {
                TransactionHash::zero()
            };
            let parent_2 = if tips.len() > 1 {
                TransactionHash::from_hex(&tips[1])?
            } else {
                parent_1.clone()
            };

            // Build, mine PoW, and sign.
            let tx = wallet.create_transaction_with_pow(
                receiver.clone(),
                amount_base,
                next_nonce,
                parent_1,
                parent_2,
            );

            println!("Submitting transaction...");
            println!("  To:     {}", receiver);
            println!("  Amount: {}", format_rly(amount_base));
            println!("  Nonce:  {}", next_nonce);

            // Submit to node.
            let submit_resp = rpc_call(
                &client,
                &rpc,
                "rly_submitTransaction",
                serde_json::json!([tx]),
            )
            .await?;

            if let Some(hash) = submit_resp.get("hash") {
                println!();
                println!("Transaction accepted!");
                println!("Hash: {}", hash.as_str().unwrap_or("?"));
            } else {
                eprintln!("Transaction rejected by node.");
                std::process::exit(1);
            }
        }

        Commands::Balance {
            address,
            rpc,
            passphrase,
        } => {
            let addr_str = match address {
                Some(a) => a,
                None => {
                    let pass = passphrase.unwrap_or_else(|| {
                        eprintln!("Error: provide --passphrase or an address argument.");
                        std::process::exit(1);
                    });
                    let wallet = Wallet::open(&cli.keystore, &pass)?;
                    wallet.address().to_string()
                }
            };

            let client = reqwest::Client::new();
            let resp = rpc_call(
                &client,
                &rpc,
                "rly_getBalance",
                serde_json::json!([addr_str]),
            )
            .await?;

            let balance = resp["balance"].as_u64().unwrap_or(0);
            let nonce = resp["nonce"].as_u64().unwrap_or(0);

            println!("Address: {}", addr_str);
            println!("Balance: {}", format_rly(balance));
            println!("Nonce:   {}", nonce);
        }

        Commands::Validate { address } => match Address::validate(&address) {
            Ok(_) => println!("Valid Relyo address."),
            Err(e) => {
                eprintln!("Invalid address: {}", e);
                std::process::exit(1);
            }
        },
    }

    Ok(())
}

/// Make a JSON-RPC 2.0 call to a node.
async fn rpc_call(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1,
    });

    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            anyhow::anyhow!("RPC request failed: {}. Is the node running at {}?", e, url)
        })?;

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("invalid RPC response: {}", e))?;

    if let Some(error) = json.get("error") {
        let msg = error["message"].as_str().unwrap_or("unknown error");
        anyhow::bail!("RPC error: {}", msg);
    }

    json.get("result")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("RPC response missing 'result' field"))
}
