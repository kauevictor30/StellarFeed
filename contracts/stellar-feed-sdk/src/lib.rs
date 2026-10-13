#![no_std]
//! # stellar-feed-sdk — Typed client for the StellarFeed oracle
//!
//! Lets any Soroban contract read Pyth-verified prices from a deployed
//! StellarFeed oracle in three lines:
//!
//! ```ignore
//! use stellar_feed_sdk::StellarFeedClient;
//!
//! let quote = StellarFeedClient::new(&env, &oracle).get_price_checked(1, 300)?;
//! // BTC/USD, at most 5 minutes old — `quote.price * 10^quote.exponent`.
//! ```
//!
//! ## Design notes
//! - `no_std`: safe to depend on from `#![no_std]` contracts.
//! - The client performs a real cross-contract call every time — prices
//!   are always read live, never cached.
//! - [`to_scaled`] converts Pyth's `(price, exponent)` pair into a fixed
//!   decimal integer without floats (deterministic on-chain math).

use soroban_sdk::{contracttype, symbol_short, Address, Env, IntoVal, Val, Vec};

/// A verified quote, mirroring the oracle's `StoredPrice`.
///
/// Soroban contract types are structural: this decodes from any contract
/// returning `{price: i64, exponent: i32, timestamp_us: u64}` under those
/// exact field names.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredPrice {
    /// Raw price. Real value = `price * 10^exponent`.
    pub price: i64,
    /// Decimal exponent (usually negative).
    pub exponent: i32,
    /// Per-feed update timestamp, microseconds since epoch.
    pub timestamp_us: u64,
}

/// Quote too old for the caller's freshness requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StalePrice {
    /// Seconds elapsed since the quote (ledger clock).
    pub age_secs: u64,
}

/// Typed client for a deployed StellarFeed oracle contract.
pub struct StellarFeedClient<'a> {
    env: &'a Env,
    contract_id: Address,
}

impl<'a> StellarFeedClient<'a> {
    /// Binds the client to an oracle contract address.
    pub fn new(env: &'a Env, contract_id: &Address) -> Self {
        Self {
            env,
            contract_id: contract_id.clone(),
        }
    }

    /// Returns the feed's latest verified quote.
    /// Traps if the oracle holds no quote for `feed_id`.
    pub fn get_price(&self, feed_id: u32) -> StoredPrice {
        let args: Vec<Val> = (feed_id,).into_val(self.env);
        self.env
            .invoke_contract(&self.contract_id, &symbol_short!("get_price"), args)
    }

    /// Same as [`get_price`](Self::get_price), but returns `Err(StalePrice)`
    /// when the quote is older than `max_age_secs` (ledger clock, so
    /// freshness never depends on off-chain time).
    pub fn get_price_checked(
        &self,
        feed_id: u32,
        max_age_secs: u64,
    ) -> Result<StoredPrice, StalePrice> {
        let quote = self.get_price(feed_id);
        let age_secs = self
            .env
            .ledger()
            .timestamp()
            .saturating_sub(quote.timestamp_us / 1_000_000);
        if age_secs > max_age_secs {
            return Err(StalePrice { age_secs });
        }
        Ok(quote)
    }

    /// Contract address this client talks to.
    pub fn contract_id(&self) -> &Address {
        &self.contract_id
    }

    /// Re-exported for convenience in signatures.
    pub fn env(&self) -> &'a Env {
        self.env
    }
}

/// Converts `(price, exponent)` into an integer with `target_decimals`
/// places (e.g. Pyth `(__, -8)` → Stellar-style 7 decimals).
/// Returns `None` on overflow instead of trapping.
///
/// Pure function — no chain access, trivially unit-testable.
pub fn to_scaled(price: i64, exponent: i32, target_decimals: u32) -> Option<i128> {
    let shift = target_decimals as i32 + exponent;
    let base = price as i128;
    if shift >= 0 {
        base.checked_mul(10i128.checked_pow(shift as u32)?)
    } else {
        base.checked_div(10i128.checked_pow(shift.unsigned_abs())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_pyth_8_decimals_to_stellar_7() {
        // 65432.10987654 → 654321098765 (7 decimals, truncated toward zero).
        assert_eq!(to_scaled(6_543_210_987_654, -8, 7), Some(654_321_098_765));
    }

    #[test]
    fn handles_positive_exponents() {
        assert_eq!(to_scaled(42, 2, 0), Some(4200));
    }

    #[test]
    fn returns_none_on_overflow() {
        assert_eq!(to_scaled(i64::MAX, 0, 30), None);
    }

    #[test]
    fn zero_price_stays_zero() {
        assert_eq!(to_scaled(0, -8, 7), Some(0));
    }
}
