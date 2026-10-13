#![no_std]
//! # StellarFeed — Price Oracle (Soroban / Rust)
//!
//! A minimal, auditable contract storing the latest quote
//! for each asset (e.g. `"BRLXLM"`, `"BRLUSDC"`).
//!
//! ## Price convention
//! - `price` is an `i128` with **7 decimal places** (factor `10_000_000`),
//!   matching Stellar's precision standard.
//!   E.g.: R$ 5.1234567 → `51_234_567`.
//! - `timestamp` is Unix seconds (ledger timestamp validated off-chain).
//!
//! ## Authorization
//! - `update_price` requires `admin.require_auth()` — only the off-chain Worker.
//! - `get_price` is read-only; any dApp can read without signing.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env, Symbol,
};

/// Fixed scale factor: 10^7.
pub const PRICE_SCALE: i128 = 10_000_000;

/// Contract version (for frontend display / upgrade checks).
pub const VERSION: u32 = 1;

/// Storage keys.
///
/// - `Admin` lives in **instance storage** (one per contract).
/// - Each price lives in **persistent storage** (one entry per symbol,
///   with its own TTL extended on every update).
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Price(Symbol),
}

/// Stored quote.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    /// Price scaled by PRICE_SCALE.
    pub price: i128,
    /// Unix timestamp (seconds) reported by the Worker.
    pub timestamp: u64,
}

/// Typed contract errors.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FeedError {
    /// `__constructor` hasn't run / Admin not set.
    NotInitialized = 1,
    /// Price must be > 0.
    InvalidPrice = 2,
    /// Zero timestamp (should never happen; guards against defaults).
    InvalidTimestamp = 3,
    /// No quote ever published for this symbol.
    NoPriceData = 4,
}

/// Event published on every update — indexable via RPC `getEvents`.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceUpdated {
    /// Updated symbol (e.g. "BRLXLM"). Becomes an event topic.
    #[topic]
    pub symbol: Symbol,
    /// Scaled price.
    pub price: i128,
    /// Timestamp reported by the Worker.
    pub timestamp: u64,
}

#[contract]
pub struct StellarFeedContract;

#[contractimpl]
impl StellarFeedContract {
    // ------------------------------------------------------------------
    // Initialization (runs once, atomically, at deploy time)
    // ------------------------------------------------------------------

    /// Initializes the contract, setting the Admin (Worker address).
    ///
    /// Called via: `stellar contract deploy ... -- --admin <ADDRESS>`
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        // Generous TTL for the instance entry (~180 days in 5s ledgers).
        env.storage().instance().extend_ttl(100_000, 1_000_000);
    }

    // ------------------------------------------------------------------
    // Writes — Admin (Worker) only
    // ------------------------------------------------------------------

    /// Records/updates a symbol's quote.
    ///
    /// # Arguments
    /// - `symbol`: e.g. `Symbol::new(&env, "BRLXLM")` (max 32 chars).
    /// - `price`: value scaled by 1e7. Must be > 0.
    /// - `timestamp`: Unix seconds. Must be > 0.
    ///
    /// # Authorization
    /// Requires the Admin's signature (`admin.require_auth()`).
    pub fn update_price(
        env: Env,
        symbol: Symbol,
        price: i128,
        timestamp: u64,
    ) -> Result<(), FeedError> {
        // 1. Authorization: Admin only.
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(FeedError::NotInitialized)?;
        admin.require_auth();

        // 2. Input validation (cheap, before writing).
        if price <= 0 {
            return Err(FeedError::InvalidPrice);
        }
        if timestamp == 0 {
            return Err(FeedError::InvalidTimestamp);
        }

        // 3. Write to persistent storage + TTL extension.
        //    (~30-day threshold / ~90-day extend-to, in ~5s ledgers:
        //     17280 ledgers ≈ 1 day.)
        let key = DataKey::Price(symbol.clone());
        let data = PriceData { price, timestamp };
        env.storage().persistent().set(&key, &data);
        env.storage()
            .persistent()
            .extend_ttl(&key, 30 * 17280, 90 * 17280);

        // 4. Event for indexers/dashboards.
        PriceUpdated {
            symbol,
            price,
            timestamp,
        }
        .publish(&env);

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

    /// Returns a symbol's latest quote.
    /// Fails with `NoPriceData` if never published.
    pub fn get_price(env: Env, symbol: Symbol) -> Result<PriceData, FeedError> {
        env.storage()
            .persistent()
            .get(&DataKey::Price(symbol))
            .ok_or(FeedError::NoPriceData)
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

// ----------------------------------------------------------------------
// Unit tests (run with `cargo test`, no network needed)
// ----------------------------------------------------------------------
#[cfg(test)]
mod test;
