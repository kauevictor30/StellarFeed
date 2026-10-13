//! Worker configuration via environment variables.
//!
//! Copy `worker/.env.example` to `worker/.env` (or export the vars)
//! and adjust to your environment.

use std::env;

use anyhow::{Context, Result};

/// Immutable worker config, resolved once at startup.
#[derive(Debug, Clone)]
pub struct Config {
    /// StellarFeed contract ID on Testnet (e.g. `CABC...`).
    pub contract_id: String,
    /// Source identity registered in `stellar keys` (the on-chain Admin).
    pub source_account: String,
    /// Stellar network: `testnet` (default) or `mainnet`.
    pub network: String,
    /// CoinGecko base URL (allows mocking in tests).
    pub coingecko_url: String,
    /// Interval between update cycles, in seconds.
    pub poll_interval_secs: u64,
    /// Price scale factor (must match the contract's `PRICE_SCALE`).
    pub price_scale: f64,
    /// If `true`, only fetch and log — never send transactions.
    pub dry_run: bool,
}

impl Config {
    /// Reads environment variables (with sensible defaults).
    pub fn from_env() -> Result<Self> {
        // Loads `.env` if present; ignores errors (vars may come from shell/CI).
        let _ = dotenvy::dotenv();

        let contract_id = env::var("STELLAR_CONTRACT_ID").context(
            "STELLAR_CONTRACT_ID not set. Deploy the contract and export its contract ID.",
        )?;
        let source_account = env::var("STELLAR_SOURCE_ACCOUNT").unwrap_or_else(|_| "alice".into());
        let network = env::var("STELLAR_NETWORK").unwrap_or_else(|_| "testnet".into());
        let coingecko_url =
            env::var("COINGECKO_URL").unwrap_or_else(|_| "https://api.coingecko.com/api/v3".into());
        let poll_interval_secs = env::var("POLL_INTERVAL_SECS")
            .unwrap_or_else(|_| "300".into())
            .parse::<u64>()
            .context("invalid POLL_INTERVAL_SECS (expected integer)")?;
        let price_scale = env::var("PRICE_SCALE")
            .unwrap_or_else(|_| "10000000".into())
            .parse::<f64>()
            .context("invalid PRICE_SCALE")?;
        let dry_run = env::var("DRY_RUN")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        Ok(Self {
            contract_id,
            source_account,
            network,
            coingecko_url,
            poll_interval_secs,
            price_scale,
            dry_run,
        })
    }
}
