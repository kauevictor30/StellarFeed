//! On-chain submission via the Stellar CLI (official tool, written in Rust).
//!
//! ## Why via CLI instead of manual XDR?
//! Building and signing an `InvokeHostFunction` by hand requires managing
//! sequence numbers, storage footprints, resource fees, simulation, and
//! ed25519 signing — all error-prone. The `stellar` CLI (official
//! `stellar-cli` crate, same team as `soroban-sdk`) already does this and is
//! the recommended path for simple bots/workers.
//!
//! The Rust worker is still the brain: it schedules, fetches prices,
//! validates, retries with backoff, and logs. The CLI is just the
//! "sign-and-send engine", invoked as a subprocess.
//!
//! Prerequisites on the worker machine:
//! 1. `stellar` CLI installed (`cargo install --locked stellar-cli`)
//! 2. Identity created and funded:
//!    `stellar keys generate alice --network testnet --fund`
//! 3. That identity is the `admin` passed at deploy (`-- --admin alice`).
//!
//! ## Future migration (fully in-process signing)
//! To drop the subprocess, replace `submit_quote` with RPC calls
//! (`getAccount` → `simulateTransaction` → sign with `ed25519-dalek`
//! + `stellar-strkey` → `sendTransaction`) using `reqwest`. This module's
//! interface (`submit_quote(&Quote)`) does not need to change.

use anyhow::{Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::{info, warn};

use crate::config::Config;
use crate::price::Quote;

/// Submits a quote by calling `update_price` on the contract.
///
/// Equivalent to:
/// ```sh
/// stellar contract invoke \
///   --id <CONTRACT_ID> --source-account <ADMIN> --network testnet \
///   -- update-price --symbol BRLXLM --price 7200000 --timestamp 1700000000
/// ```
pub async fn submit_quote(cfg: &Config, q: &Quote) -> Result<String> {
    if cfg.dry_run {
        info!(
            symbol = q.symbol,
            price = q.scaled_price,
            "[dry-run] would skip on-chain submission"
        );
        return Ok("dry-run".to_string());
    }

    // Simple backoff: 3 attempts (2s, 4s, 8s).
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=3 {
        match invoke_once(cfg, q).await {
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
async fn invoke_once(cfg: &Config, q: &Quote) -> Result<String> {
    let price = q.scaled_price.to_string();
    let timestamp = q.timestamp.to_string();

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
        "--symbol",
        q.symbol,
        "--price",
        &price,
        "--timestamp",
        &timestamp,
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
        info!(symbol = q.symbol, "update_price confirmed: {stdout}");
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
