//! On-chain relay via the Stellar CLI (official tool, written in Rust).
//!
//! The worker holds NO price authority: it forwards opaque Pyth-signed
//! bytes and the contract verifies them. Forged input fails closed
//! on-chain, so a compromised worker can at worst withhold updates
//! (liveness), never invent prices (safety).
//!
//! Prerequisites on the worker machine:
//! 1. `stellar` CLI installed (`cargo install --locked stellar-cli`)
//! 2. Identity created and funded (pays fees only):
//!    `stellar keys generate alice --network testnet --fund`
//!
//! ## Future migration (fully in-process relay)
//! Replace `submit_payload` with RPC calls (`getAccount` → build
//! `InvokeHostFunction` with the payload as `Bytes` → sign with
//! `ed25519-dalek` → `sendTransaction`) using `reqwest`. The
//! `submit_payload(&str)` interface does not need to change.

use anyhow::{Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::{info, warn};

use crate::config::Config;

/// Relays one signed payload by calling `update_price` on the contract.
///
/// Equivalent to:
/// ```sh
/// stellar contract invoke \
///   --id <CONTRACT_ID> --source-account <ADMIN> --network testnet \
///   -- update-price --payload <HEX>
/// ```
/// (`Bytes` args are passed as hex — see `stellar contract invoke --help`.)
pub async fn submit_payload(cfg: &Config, payload_hex: &str) -> Result<String> {
    if cfg.dry_run {
        info!(
            bytes = payload_hex.len() / 2,
            "[dry-run] would relay payload on-chain"
        );
        return Ok("dry-run".to_string());
    }

    // Backoff: 3 attempts (2s, 4s, 8s).
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=3 {
        match invoke_once(cfg, payload_hex).await {
            Ok(out) => return Ok(out),
            Err(e) => {
                warn!("invoke failed (attempt {attempt}/3): {e:#}");
                last_err = Some(e);
                tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt))).await;
            }
        }
    }
    Err(last_err.unwrap())
}

/// A single `stellar contract invoke` attempt.
async fn invoke_once(cfg: &Config, payload_hex: &str) -> Result<String> {
    let mut cmd = Command::new("stellar");
    cmd.args([
        "contract",
        "invoke",
        "--id",
        &cfg.contract_id,
        "--source-account",
        &cfg.source_account,
        "--network",
        &cfg.network,
        "--",
        "update-price",
        "--payload",
        payload_hex,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let output = tokio::time::timeout(Duration::from_secs(60), cmd.output())
        .await
        .context("timed out after 60s waiting for the stellar CLI")?
        .context("failed to run `stellar` — is it installed on PATH?")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info!("update_price confirmed: {stdout}");
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("stellar CLI exit {}: {}", output.status, stderr.trim())
    }
}

/// Startup health check: CLI present + RPC reachable.
pub async fn preflight_check(rpc_url: &str) -> Result<()> {
    // 1. CLI installed?
    let out = Command::new("stellar")
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .await
        .context("`stellar` CLI not found on PATH")?;
    info!(
        "stellar CLI: {}",
        String::from_utf8_lossy(&out.stdout).trim()
    );

    // 2. Does the RPC answer `getHealth`?
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let res: serde_json::Value = client
        .post(rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "getHealth", "params": []
        }))
        .send()
        .await
        .context("RPC unreachable")?
        .json()
        .await
        .context("invalid RPC response")?;
    info!("RPC health: {res}");
    Ok(())
}

/// Public RPC URL from the network name.
pub fn rpc_url_for(network: &str) -> &'static str {
    match network {
        "mainnet" => "https://mainnet.sorobanrpc.com",
        "futurenet" => "https://rpc-futurenet.stellar.org",
        _ => "https://soroban-testnet.stellar.org",
    }
}

/// `pyth-lazer-stellar` verifier address for the network.
pub fn lazer_verifier_for(network: &str) -> &'static str {
    match network {
        "mainnet" => "CACZ3GBAKUPIAFRILUFO27J5RUH5GJ2VSJ46LP6GJYSKGDRTQ5MS3HCH",
        _ => "CAYFT5JE3UQTKT4Q6ZOZK4FXVYVT6RE3MFC7STA4UB6WAEGBT65MRU52",
    }
}
