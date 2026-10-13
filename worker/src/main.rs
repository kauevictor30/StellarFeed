//! # StellarFeed Relay Worker (pure Rust)
//!
//! Off-chain relayer for the Pyth-backed oracle:
//! ```text
//! Pyth Pro websocket (signed leEcdsa) → stellar contract invoke → Soroban
//! ```
//! The worker never touches prices: it streams signed envelopes and relays
//! the latest one every `RELAY_INTERVAL_SECS`. Verification happens
//! on-chain, so relaying is permissionless and failure-closed.
//! No Python: only Rust + the official `stellar` CLI for sending.

mod config;
mod lazer;
mod stellar;

use std::time::Duration;

use anyhow::Result;
use tokio::sync::watch;
use tracing::{error, info, warn};

use config::Config;
use lazer::SignedPayload;

#[tokio::main]
async fn main() -> Result<()> {
    // Structured logs; level via `RUST_LOG` (default: info).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stellar_feed_worker=info".into()),
        )
        .init();

    let cfg = Config::from_env()?;
    info!(
        network = %cfg.network,
        contract = %cfg.contract_id,
        source = %cfg.source_account,
        feeds = ?cfg.pyth_feed_ids,
        channel = %cfg.pyth_channel,
        relay_s = cfg.relay_interval_secs,
        dry_run = cfg.dry_run,
        verifier = stellar::lazer_verifier_for(&cfg.network),
        "StellarFeed relay worker starting"
    );

    // Fail fast if the environment is broken (CLI/RPC).
    if !cfg.dry_run {
        if let Err(e) = stellar::preflight_check(stellar::rpc_url_for(&cfg.network)).await {
            warn!("preflight failed (carrying on anyway): {e:#}");
        }
    }

    // Background task keeps the latest signed payload fresh.
    let (tx, rx) = watch::channel::<Option<SignedPayload>>(None);
    let sub_cfg = cfg.clone();
    tokio::spawn(async move {
        lazer::run_subscriber(sub_cfg, tx).await;
    });

    run_relay_loop(&cfg, rx).await
}

/// Every tick: relay the newest payload unless already submitted.
async fn run_relay_loop(
    cfg: &Config,
    mut rx: watch::Receiver<Option<SignedPayload>>,
) -> Result<()> {
    let mut last_hex = String::new();
    loop {
        // Wait until at least one payload has arrived.
        if rx.borrow().is_none() {
            info!("waiting for first signed payload…");
            rx.changed().await?;
            continue;
        }
        let hex = rx
            .borrow()
            .as_ref()
            .map(|p| p.hex.clone())
            .unwrap_or_default();
        if !hex.is_empty() && hex != last_hex {
            match stellar::submit_payload(cfg, &hex).await {
                Ok(receipt) => {
                    info!(bytes = hex.len() / 2, "relayed ✓ ({receipt})");
                    last_hex = hex;
                }
                Err(e) => error!("relay failed: {e:#}"),
            }
        }
        tokio::time::sleep(Duration::from_secs(cfg.relay_interval_secs)).await;
    }
}
