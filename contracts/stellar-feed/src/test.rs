//! StellarFeed unit tests.
//! Run with: `cargo test -p stellar-feed`

use super::*;
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal, Symbol,
};

/// Helper: registers the contract with a generated admin.
fn setup() -> (Env, Address, Address, StellarFeedContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(StellarFeedContract, (admin.clone(),));
    let client = StellarFeedContractClient::new(&env, &contract_id);
    (env, admin, contract_id, client)
}

#[test]
fn test_constructor_sets_admin() {
    let (env, admin, _, client) = setup();
    let _ = &env;
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.version(), 1);
}

#[test]
fn test_update_and_get_price() {
    let (env, _, _, client) = setup();
    let symbol = Symbol::new(&env, "BRLXLM");

    client.update_price(&symbol, 1_234_567, 1_700_000_000);

    let data = client.get_price(&symbol);
    assert_eq!(data.price, 1_234_567);
    assert_eq!(data.timestamp, 1_700_000_000);
}

#[test]
fn test_update_overwrites_previous() {
    let (env, _, _, client) = setup();
    let symbol = Symbol::new(&env, "BRLUSDC");

    client.update_price(&symbol, 5_000_0000, 1_700_000_000);
    client.update_price(&symbol, 5_100_0000, 1_700_000_600);

    let data = client.get_price(&symbol);
    assert_eq!(data.price, 5_100_0000);
    assert_eq!(data.timestamp, 1_700_000_600);
}

#[test]
fn test_rejects_zero_price() {
    let (env, _, _, client) = setup();
    let symbol = Symbol::new(&env, "BRLXLM");

    let res = client.try_update_price(&symbol, &0, &1_700_000_000);
    assert_eq!(res, Err(Ok(FeedError::InvalidPrice)));
}

#[test]
fn test_rejects_zero_timestamp() {
    let (env, _, _, client) = setup();
    let symbol = Symbol::new(&env, "BRLXLM");

    let res = client.try_update_price(&symbol, &100, &0);
    assert_eq!(res, Err(Ok(FeedError::InvalidTimestamp)));
}

#[test]
fn test_get_unknown_symbol_fails() {
    let (env, _, _, client) = setup();
    let symbol = Symbol::new(&env, "UNKNOWN");

    let res = client.try_get_price(&symbol);
    assert_eq!(res, Err(Ok(FeedError::NoPriceData)));
}

#[test]
fn test_only_admin_can_update() {
    let env = Env::default();
    // NOTE: no mock_all_auths — we authorize explicitly as admin
    // and prove that an attacker gets an auth error.
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let contract_id = env.register(StellarFeedContract, (admin.clone(),));
    let client = StellarFeedContractClient::new(&env, &contract_id);
    let symbol = Symbol::new(&env, "BRLXLM");

    // Real admin: authorizes and succeeds.
    client
        .mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "update_price",
                args: (&symbol, 100i128, 1_700_000_000u64).into_val(&env),
                sub_invocations: &[],
            },
        }])
        .update_price(&symbol, &100, &1_700_000_000);

    // Attacker impersonating the admin: the host rejects it (it is not the
    // admin recorded in storage, so require_auth fails real verification).
    // Here we simulate the call with the attacker's auth — the contract calls
    // `admin.require_auth()`, and since the mock only authorizes `attacker`,
    // the host returns an auth error (HostError, not FeedError).
    let res = client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "update_price",
                args: (&symbol, 200i128, 1_700_000_001u64).into_val(&env),
                sub_invocations: &[],
            },
        }])
        .try_update_price(&symbol, &200, &1_700_000_001);
    assert!(res.is_err(), "attacker must not be able to update");
}

#[test]
fn test_set_admin_rotation() {
    let (env, _, _, client) = setup();
    let new_admin = Address::generate(&env);

    client.set_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn test_emits_price_updated_event() {
    let (env, _, _, client) = setup();
    let symbol = Symbol::new(&env, "BRLXLM");

    client.update_price(&symbol, 42_000_000, 1_700_000_000);

    // The PriceUpdated event must have been published.
    let events = env.events().all();
    assert!(!events.is_empty(), "expected at least 1 event");
}
