//! # StellarFeed Worker (pure Rust)
//!
//! Off-chain daemon feeding the oracle:
//! ```text
//! CoinGecko (HTTP) → validate/scale → stellar contract invoke (Testnet)
//! ```
//! Infinite loop with a configurable interval (`POLL_INTERVAL_SECS`).
//! No Python: only Rust + the official `stellar` CLI for signing/sending.

mod config;
mod price;
mod stellar;

use std::time::Duration;

use anyhow::Result;
use tracing::{error, info, warn};

use config::Config;

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
        interval_s = cfg.poll_interval_secs,
        dry_run = cfg.dry_run,
        "StellarFeed worker starting"
    );

    // Fail fast if the environment is broken (CLI/RPC).
    if !cfg.dry_run {
        if let Err(e) = stellar::preflight_check(stellar::rpc_url_for(&cfg.network)).await {
            warn!("preflight failed (carrying on anyway): {e:#}");
        }
    }

    let http = reqwest::Client::builder()
        .user_agent("stellar-feed-worker/0.1")
        .timeout(Duration::from_secs(20))
        .build()?;

    // Main loop: fetch → publish each symbol → sleep.
    loop {
        if let Err(e) = run_cycle(&cfg, &http).await {
            error!("cycle failed: {e:#}");
        }
        info!("next cycle in {}s", cfg.poll_interval_secs);
        tokio::time::sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

/// One full cycle: fetch prices and publish each symbol.
async fn run_cycle(cfg: &Config, http: &reqwest::Client) -> Result<()> {
    let quotes = price::fetch_quotes(http, &cfg.coingecko_url, cfg.price_scale).await?;

    for q in &quotes {
        match stellar::submit_quote(cfg, q).await {
            Ok(receipt) => info!(
                symbol = q.symbol,
                human = q.human_price,
                scaled = q.scaled_price,
                "published ✓ ({receipt})"
            ),
            Err(e) => error!(symbol = q.symbol, "publish failed: {e:#}"),
        }
        // Small pause between symbols to avoid RPC rate limits.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Ok(())
}
