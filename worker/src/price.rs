//! Price fetching via CoinGecko (public API, no key).
//!
//! Strategy:
//! - `GET /simple/price?ids=stellar,usd-coin&vs_currencies=brl`
//!   returns the BRL price of 1 XLM and 1 USDC.
//! - We convert to scaled integers (`price * PRICE_SCALE`) to
//!   send to the contract (which only works with `i128`).
//!
//! On-chain symbols published by the worker:
//! - `"BRLXLM"`  — BRL per 1 XLM
//! - `"BRLUSDC"` — BRL per 1 USDC

use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;
use std::collections::HashMap;
use tracing::{info, warn};

/// A quote ready to go on-chain.
#[derive(Debug, Clone)]
pub struct Quote {
    /// On-chain symbol (e.g. `"BRLXLM"`).
    pub symbol: &'static str,
    /// Scaled price (e.g. R$ 5.12 → `51_200_000` at 1e7 scale).
    pub scaled_price: i128,
    /// "Human" price (float, for logs only).
    pub human_price: f64,
    /// Unix timestamp (seconds) of the fetch.
    pub timestamp: u64,
}

/// CoinGecko response: `{ "stellar": { "brl": 0.72 }, ... }`.
type SimplePrice = HashMap<String, HashMap<String, f64>>;

/// Fetches prices from CoinGecko with retry + exponential backoff.
pub async fn fetch_quotes(client: &Client, base_url: &str, price_scale: f64) -> Result<Vec<Quote>> {
    let url = format!(
        "{}/simple/price?ids=stellar,usd-coin&vs_currencies=brl",
        base_url.trim_end_matches('/')
    );

    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=4 {
        match client.get(&url).send().await {
            Ok(resp) => {
                if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    warn!("CoinGecko rate limit (429), attempt {attempt}/4 — waiting…");
                    tokio::time::sleep(std::time::Duration::from_secs(5 * attempt as u64)).await;
                    continue;
                }
                let resp = resp
                    .error_for_status()
                    .context("CoinGecko returned an HTTP error")?;
                let data: SimplePrice = resp.json().await.context("invalid CoinGecko JSON")?;
                return build_quotes(&data, price_scale);
            }
            Err(e) => {
                warn!("HTTP failure (attempt {attempt}/4): {e:#}");
                last_err = Some(e.into());
                tokio::time::sleep(std::time::Duration::from_secs(2_u64.pow(attempt))).await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("CoinGecko unreachable")))
}

/// Converts the raw response into scaled `Quote`s.
fn build_quotes(data: &SimplePrice, price_scale: f64) -> Result<Vec<Quote>> {
    // Maps CoinGecko id -> on-chain symbol.
    const PAIRS: [(&str, &str); 2] = [("stellar", "BRLXLM"), ("usd-coin", "BRLUSDC")];

    let timestamp = Utc::now().timestamp() as u64;
    let mut out = Vec::with_capacity(PAIRS.len());

    for (coin_id, symbol) in PAIRS {
        let human_price = data
            .get(coin_id)
            .and_then(|m| m.get("brl"))
            .copied()
            .with_context(|| format!("CoinGecko did not return `{coin_id}/brl`"))?;

        if !(human_price.is_finite() && human_price > 0.0) {
            anyhow::bail!("invalid price for {coin_id}: {human_price}");
        }

        let scaled_price = (human_price * price_scale).round() as i128;
        info!(symbol, human_price, scaled_price, "quote fetched");
        out.push(Quote {
            symbol,
            scaled_price,
            human_price,
            timestamp,
        });
    }
    Ok(out)
}

/// Converts float → scaled integer (used in tests and reused elsewhere).
pub fn scale_price(human: f64, scale: f64) -> anyhow::Result<i128> {
    if !(human.is_finite() && human > 0.0) {
        anyhow::bail!("invalid price: {human}");
    }
    Ok((human * scale).round() as i128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_brl_price_to_7_decimals() {
        // R$ 5.1234567 → 51_234_567
        assert_eq!(scale_price(5.1234567, 10_000_000.0).unwrap(), 51_234_567);
    }

    #[test]
    fn rejects_non_positive() {
        assert!(scale_price(0.0, 10_000_000.0).is_err());
        assert!(scale_price(-1.5, 10_000_000.0).is_err());
    }

    #[test]
    fn builds_quotes_from_mock_payload() {
        let mut data: SimplePrice = HashMap::new();
        data.insert("stellar".into(), HashMap::from([("brl".into(), 0.72)]));
        data.insert("usd-coin".into(), HashMap::from([("brl".into(), 5.41)]));

        let quotes = build_quotes(&data, 10_000_000.0).unwrap();
        assert_eq!(quotes.len(), 2);
        assert_eq!(quotes[0].symbol, "BRLXLM");
        assert_eq!(quotes[0].scaled_price, 7_200_000);
        assert_eq!(quotes[1].symbol, "BRLUSDC");
    }
}
