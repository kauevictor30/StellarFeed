//! demo-consumer tests: SDK client round-trip against a mock oracle.
//!
//! The mock stands in for a deployed StellarFeed: because Soroban
//! contract types are structural, the SDK decodes the mock's
//! `{price, exponent, timestamp_us}` map exactly as it would the real
//! oracle's. This proves the cross-contract wiring end to end through
//! the host. Run with: `cargo test -p demo-consumer`

use super::*;
use soroban_sdk::{contract as contract_attr, contractimpl as impl_attr, Env};

// --- Mock oracle ---------------------------------------------------------
// Mirrors the real oracle's `get_price` surface with canned data.

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MockQuote {
    pub price: i64,
    pub exponent: i32,
    pub timestamp_us: u64,
}

#[contract_attr]
pub struct MockFeed;

#[impl_attr]
impl MockFeed {
    /// Canned BTC/USD quote: 65432.10987654 at 8 decimals.
    pub fn get_price(_env: Env, feed_id: u32) -> MockQuote {
        assert_eq!(feed_id, 1, "mock only serves feed 1");
        MockQuote {
            price: 6_543_210_987_654,
            exponent: -8,
            timestamp_us: 1_700_000_000_000_000,
        }
    }
}

// --- Tests ---------------------------------------------------------------

/// Registers [mock oracle + demo] and returns a demo client.
fn setup() -> (Env, DemoConsumerClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let mock_id = env.register(MockFeed, ());
    let demo_id = env.register(DemoConsumer, (mock_id.clone(),));
    // Silence unused-var style lints while keeping the addresses visible.
    let _ = mock_id;
    (env, DemoConsumerClient::new(&env, &demo_id))
}

#[test]
fn test_read_price_round_trip() {
    let (_env, client) = setup();

    let quote = client.read_price(&1u32);
    assert_eq!(quote.price, 6_543_210_987_654);
    assert_eq!(quote.exponent, -8);
    assert_eq!(quote.timestamp_us, 1_700_000_000_000_000);
}

#[test]
fn test_read_price_scaled_to_7_decimals() {
    let (_env, client) = setup();

    // 65432.10987654 → 654321098765 (truncated toward zero).
    assert_eq!(
        client.read_price_scaled(&1u32, &7u32),
        Some(654_321_098_765)
    );
}

#[test]
fn test_oracle_address_pinned_at_deploy() {
    let (env, _) = setup();
    let mock_id = env.register(MockFeed, ());
    let demo_id = env.register(DemoConsumer, (mock_id.clone(),));
    let stored: Address = env.as_contract(&demo_id, || {
        env.storage().instance().get(&DataKey::Oracle).unwrap()
    });
    assert_eq!(stored, mock_id);
}
