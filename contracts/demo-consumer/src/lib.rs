#![no_std]
//! # demo-consumer — Minimal StellarFeed integration example
//!
//! A tiny contract proving that any third-party dApp can read Pyth-verified
//! prices from a deployed StellarFeed oracle using only `stellar-feed-sdk`:
//!
//! ```text
//! DemoConsumer --cross-contract--> StellarFeed oracle --verified--> Pyth
//! ```
//!
//! Copy this pattern into your own contract: store the oracle address once,
//! then call through the typed client.

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
use stellar_feed_sdk::{to_scaled, StellarFeedClient, StoredPrice};

/// Storage keys (instance: one oracle address per deployment).
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Oracle,
}

#[contract]
pub struct DemoConsumer;

#[contractimpl]
impl DemoConsumer {
    /// Pins the StellarFeed oracle this demo reads from.
    /// Run once at deploy: `stellar contract deploy ... -- --oracle <ORACLE_ID>`
    pub fn __constructor(env: Env, oracle: Address) {
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        env.storage().instance().extend_ttl(100_000, 1_000_000);
    }

    /// Returns the raw verified quote for a Pyth feed ID (e.g. BTC/USD = 1).
    pub fn read_price(env: Env, feed_id: u32) -> StoredPrice {
        Self::client(&env).get_price(feed_id)
    }

    /// Returns the quote rescaled to `decimals` places (e.g. `7` for
    /// Stellar-style precision), or `None` on arithmetic overflow.
    pub fn read_price_scaled(env: Env, feed_id: u32, decimals: u32) -> Option<i128> {
        let quote = Self::client(&env).get_price(feed_id);
        to_scaled(quote.price, quote.exponent, decimals)
    }

    /// Binds the SDK client to the pinned oracle address.
    fn client(env: &Env) -> StellarFeedClient<'_> {
        let oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .expect("oracle not set — deploy with `-- --oracle <ID>`");
        StellarFeedClient::new(env, &oracle)
    }
}

#[cfg(test)]
mod test;
