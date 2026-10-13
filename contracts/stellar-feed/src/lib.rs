#![no_std]
//! # StellarFeed — Pyth Lazer-backed Price Oracle (Soroban / Rust)
//!
//! A trust-minimized price feed: instead of trusting an off-chain worker's
//! HTTP fetch, this contract only stores prices carried in **Pyth-signed
//! payloads**, verified on-chain by the official `pyth-lazer-stellar`
//! verifier contract (cross-contract call).
//!
//! ## Trust model
//! - Anyone (typically our relay worker) may submit `payload: Bytes` —
//!   relaying is permissionless because verification is the trust boundary.
//!   Never trust a parsed update handed in by the caller; always
//!   `verify_update` inside this contract (Pyth's own rule).
//! - The Admin exists only for configuration (`set_lazer`, `set_admin`).
//! - Freshness is enforced on-chain: `get_price_checked` compares the
//!   feed's `feed_update_timestamp` against the ledger timestamp, so
//!   consumers never silently read stale quotes.
//!
//! ## Data model
//! Prices are keyed by Pyth **feed ID** (`u32`, e.g. BTC/USD = 1 — see the
//! [Price Feed IDs](https://docs.pyth.network/price-feeds/pro/price-feed-ids)
//! page). Each record keeps the raw `(price, exponent)` pair plus the
//! per-feed update timestamp in microseconds.

use pyth_lazer_stellar_sdk::{PythLazerClient, VerifyError};
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Bytes, Env,
};

/// Contract version (for frontend display / upgrade checks).
pub const VERSION: u32 = 2;

/// Storage keys.
///
/// - `Lazer` / `Admin` live in **instance storage** (one per contract).
/// - Each verified quote lives in **persistent storage** (one entry per
///   feed ID, TTL extended on every update).
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Lazer,
    Admin,
    Price(u32),
}

/// A verified quote, stored as published by Pyth.
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

/// Typed contract errors.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FeedError {
    /// `__constructor` hasn't run / verifier or admin not set.
    NotInitialized = 1,
    /// The payload failed on-chain verification (untrusted signer,
    /// expired signer, or malformed envelope).
    VerifyFailed = 2,
    /// No quote ever published for this feed ID.
    NoPriceData = 3,
    /// Quote is older than the caller's `max_age_secs`.
    StalePrice = 4,
}

/// Event published per stored feed — indexable via RPC `getEvents`.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceUpdated {
    /// Pyth feed ID. Becomes an event topic.
    #[topic]
    pub feed_id: u32,
    /// Raw verified price.
    pub price: i64,
    /// Decimal exponent.
    pub exponent: i32,
    /// Per-feed update timestamp (µs).
    pub timestamp_us: u64,
}

#[contract]
pub struct StellarFeedContract;

#[contractimpl]
impl StellarFeedContract {
    // ------------------------------------------------------------------
    // Initialization (runs once, atomically, at deploy time)
    // ------------------------------------------------------------------

    /// Initializes the contract.
    ///
    /// - `lazer`: address of the deployed `pyth-lazer-stellar` verifier
    ///   (testnet `CAYFT5JE3UQTKT4Q6ZOZK4FXVYVT6RE3MFC7STA4UB6WAEGBT65MRU52`,
    ///   mainnet `CACZ3GBAKUPIAFRILUFO27J5RUH5GJ2VSJ46LP6GJYSKGDRTQ5MS3HCH`).
    /// - `admin`: configuration admin (rotation only, cannot forge prices).
    ///
    /// Deploy via:
    /// `stellar contract deploy ... -- --lazer <VERIFIER> --admin <ADMIN>`
    pub fn __constructor(env: Env, lazer: Address, admin: Address) {
        env.storage().instance().set(&DataKey::Lazer, &lazer);
        env.storage().instance().set(&DataKey::Admin, &admin);
        // Generous TTL for the instance entry (~180 days in 5s ledgers).
        env.storage().instance().extend_ttl(100_000, 1_000_000);
    }

    // ------------------------------------------------------------------
    // Writes — permissionless relay, verified on-chain
    // ------------------------------------------------------------------

    /// Relays a Pyth-signed Lazer payload (`leEcdsa` format, hex-decoded
    /// by the caller into `Bytes`). Verifies it via the trusted verifier
    /// contract and stores every feed that carries a full
    /// `(price, exponent, feed_update_timestamp)` triple.
    ///
    /// No authentication: cryptographic verification replaces the admin
    /// signature. A forged or tampered payload fails closed.
    pub fn update_price(env: Env, payload: Bytes) -> Result<(), FeedError> {
        let lazer: Address = env
            .storage()
            .instance()
            .get(&DataKey::Lazer)
            .ok_or(FeedError::NotInitialized)?;

        // The trust boundary: verify + parse in one cross-contract call.
        let update = PythLazerClient::new(&env, &lazer).verify_update(&payload);

        let update = update.map_err(|_: VerifyError| FeedError::VerifyFailed)?;

        // Store each fully-populated feed. Feeds missing a requested
        // property decode as `None` — skip them instead of trapping.
        for feed in update.feeds.iter() {
            let (Some(price), Some(exponent_raw), Some(timestamp_us)) =
                (feed.price, feed.exponent, feed.feed_update_timestamp)
            else {
                continue;
            };
            let feed_id = feed.feed_id as u32;
            let record = StoredPrice {
                price,
                exponent: i32::from(exponent_raw),
                timestamp_us,
            };

            // ~30-day threshold / ~90-day extend-to, in ~5s ledgers
            // (17280 ledgers ≈ 1 day).
            let key = DataKey::Price(feed_id);
            env.storage().persistent().set(&key, &record);
            env.storage()
                .persistent()
                .extend_ttl(&key, 30 * 17280, 90 * 17280);

            PriceUpdated {
                feed_id,
                price,
                exponent: record.exponent,
                timestamp_us,
            }
            .publish(&env);
        }
        Ok(())
    }

    /// Points the contract at a new verifier. Admin only.
    pub fn set_lazer(env: Env, new_lazer: Address) -> Result<(), FeedError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(FeedError::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Lazer, &new_lazer);
        env.storage().instance().extend_ttl(100_000, 1_000_000);
        Ok(())
    }

    /// Rotates the Admin. Only the current Admin can call it.
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), FeedError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(FeedError::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.storage().instance().extend_ttl(100_000, 1_000_000);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Reads — public, no authentication (used by dApps and frontend)
    // ------------------------------------------------------------------

    /// Returns a feed's latest verified quote.
    /// Fails with `NoPriceData` if never published.
    pub fn get_price(env: Env, feed_id: u32) -> Result<StoredPrice, FeedError> {
        env.storage()
            .persistent()
            .get(&DataKey::Price(feed_id))
            .ok_or(FeedError::NoPriceData)
    }

    /// Same as `get_price`, but fails with `StalePrice` when the quote is
    /// older than `max_age_secs` (measured against the ledger timestamp,
    /// so freshness never depends on off-chain clocks).
    pub fn get_price_checked(
        env: Env,
        feed_id: u32,
        max_age_secs: u64,
    ) -> Result<StoredPrice, FeedError> {
        let record: StoredPrice = env
            .storage()
            .persistent()
            .get(&DataKey::Price(feed_id))
            .ok_or(FeedError::NoPriceData)?;
        let now_secs = env.ledger().timestamp();
        if is_stale(record.timestamp_us, now_secs, max_age_secs) {
            return Err(FeedError::StalePrice);
        }
        Ok(record)
    }

    /// Returns the configured verifier address.
    pub fn get_lazer(env: Env) -> Result<Address, FeedError> {
        env.storage()
            .instance()
            .get(&DataKey::Lazer)
            .ok_or(FeedError::NotInitialized)
    }

    /// Returns the current Admin address.
    pub fn get_admin(env: Env) -> Result<Address, FeedError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(FeedError::NotInitialized)
    }

    /// Contract version (useful for frontend/worker compatibility).
    pub fn version() -> u32 {
        VERSION
    }
}

/// Pure freshness helper (unit-testable without chain state).
fn is_stale(timestamp_us: u64, now_secs: u64, max_age_secs: u64) -> bool {
    now_secs.saturating_sub(timestamp_us / 1_000_000) > max_age_secs
}

// ----------------------------------------------------------------------
// Unit tests (run with `cargo test`, no network needed)
// ----------------------------------------------------------------------
#[cfg(test)]
mod test;
