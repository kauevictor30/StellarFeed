//! Worker configuration via environment variables.
//!
//! Copy `worker/.env.example` to `worker/.env` (or export the vars)
//! and adjust to your environment. You need a Pyth Pro API key — see
//! https://docs.pyth.network/price-feeds/pro/subscribe-to-prices
//! ("Acquire an API key"). Never commit the key.

use std::env;

use anyhow::{Context, Result};

/// Immutable worker config, resolved once at startup.
#[derive(Debug, Clone)]
pub struct Config {
    /// StellarFeed contract ID on Stellar (output of `stellar contract deploy`).
    pub contract_id: String,
    /// Source identity registered in `stellar keys` (pays fees, no price authority).
    pub source_account: String,
    /// Stellar network: `testnet` (default) or `mainnet`.
    pub network: String,
    /// Pyth Pro websocket endpoint.
    pub pyth_ws_url: String,
    /// Pyth Pro API key (`Authorization: Bearer`).
    pub pyth_api_key: String,
    /// Pyth feed IDs to subscribe to (e.g. `1,2` for BTC/USD, ETH/USD —
    /// see https://docs.pyth.network/price-feeds/pro/price-feed-ids).
    pub pyth_feed_ids: Vec<u32>,
    /// Lazer channel, e.g. `fixed_rate@1000ms`.
    pub pyth_channel: String,
    /// Minimum seconds between on-chain relays (chain writes are far
    /// slower and costlier than the websocket tick).
    pub relay_interval_secs: u64,
    /// If `true`, only receive and log — never send transactions.
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
        let pyth_ws_url = env::var("PYTH_WS_URL")
            .unwrap_or_else(|_| "wss://pyth-lazer-0.dourolabs.app/v1/stream".into());
        let pyth_api_key = env::var("PYTH_API_KEY").context(
            "PYTH_API_KEY not set. Get one at https://docs.pyth.network/price-feeds/pro/subscribe-to-prices",
        )?;
        let pyth_feed_ids = env::var("PYTH_FEED_IDS")
            .unwrap_or_else(|_| "1,2".into())
            .split(',')
            .map(|s| {
                s.trim()
                    .parse::<u32>()
                    .with_context(|| format!("invalid feed ID in PYTH_FEED_IDS: `{s}`"))
            })
            .collect::<Result<Vec<_>>>()?;
        if pyth_feed_ids.is_empty() {
            anyhow::bail!("PYTH_FEED_IDS is empty — subscribe to at least one feed");
        }
        let pyth_channel = env::var("PYTH_CHANNEL").unwrap_or_else(|_| "fixed_rate@1000ms".into());
        let relay_interval_secs = env::var("RELAY_INTERVAL_SECS")
            .unwrap_or_else(|_| "60".into())
            .parse::<u64>()
            .context("invalid RELAY_INTERVAL_SECS (expected integer)")?;
        let dry_run = env::var("DRY_RUN")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        Ok(Self {
            contract_id,
            source_account,
            network,
            pyth_ws_url,
            pyth_api_key,
            pyth_feed_ids,
            pyth_channel,
            relay_interval_secs,
            dry_run,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_feed_id_csv() {
        let ids: Vec<u32> = "1, 2,42"
            .split(',')
            .map(|s| s.trim().parse::<u32>().unwrap())
            .collect();
        assert_eq!(ids, vec![1, 2, 42]);
    }
}
