//! StellarFeed unit tests (Pyth-backed).
//! Run with: `cargo test -p stellar-feed`
//!
//! NOTE: `update_price` success paths need a genuinely Pyth-signed payload,
//! which cannot be forged in unit tests. Those paths are covered by
//! fork/integration tests with recorded mainnet payloads (see backlog).
//! Here we cover: initialization, error paths (including rejection of
//! unsigned garbage), admin rotation, and the pure staleness helper.

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, MockAuth, MockAuthInvoke},
    Address, Bytes, Env, IntoVal,
};

/// Helper: registers the contract with generated lazer + admin addresses.
/// Returns the env plus addresses; each test builds its own client from
/// them (generated clients borrow the env, so they must live in the
/// test's own scope).
fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let lazer = Address::generate(&env);
    let admin = Address::generate(&env);
    let contract_id = env.register(StellarFeedContract, (lazer.clone(), admin.clone()));
    (env, lazer, admin, contract_id)
}

#[test]
fn test_constructor_sets_lazer_and_admin() {
    let (env, lazer, admin, contract_id) = setup();
    let client = StellarFeedContractClient::new(&env, &contract_id);
    assert_eq!(client.get_lazer(), lazer);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.version(), 2);
}

#[test]
fn test_get_unknown_feed_fails() {
    let (env, _, _, contract_id) = setup();
    let client = StellarFeedContractClient::new(&env, &contract_id);

    let res = client.try_get_price(&1u32);
    assert_eq!(res, Err(Ok(FeedError::NoPriceData)));

    let res = client.try_get_price_checked(&1u32, &300u64);
    assert_eq!(res, Err(Ok(FeedError::NoPriceData)));
}

#[test]
fn test_rejects_unsigned_garbage_payload() {
    let (env, _, _, contract_id) = setup();
    let client = StellarFeedContractClient::new(&env, &contract_id);

    // Random bytes are not a valid leEcdsa envelope: the verifier must
    // reject them and the contract must fail closed with VerifyFailed.
    let garbage = Bytes::from_slice(&env, &[0xABu8; 64]);
    let res = client.try_update_price(&garbage);
    assert_eq!(res, Err(Ok(FeedError::VerifyFailed)));
}

#[test]
fn test_only_admin_can_rotate_config() {
    let env = Env::default();
    // NOTE: no mock_all_auths — authorize explicitly as admin and prove
    // that an attacker cannot rotate the verifier address.
    let lazer = Address::generate(&env);
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let new_lazer = Address::generate(&env);
    let contract_id = env.register(StellarFeedContract, (lazer, admin.clone()));
    let client = StellarFeedContractClient::new(&env, &contract_id);

    // Real admin: authorizes and succeeds.
    client
        .mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "set_lazer",
                args: (new_lazer.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .set_lazer(&new_lazer);
    assert_eq!(client.get_lazer(), new_lazer);

    // Attacker: the host rejects the auth (not the recorded admin).
    let res = client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "set_lazer",
                args: (attacker.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_set_lazer(&attacker);
    assert!(res.is_err(), "attacker must not rotate the verifier");
}

#[test]
fn test_set_admin_rotation() {
    let (env, _, _, contract_id) = setup();
    let client = StellarFeedContractClient::new(&env, &contract_id);
    let new_admin = Address::generate(&env);

    client.set_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn test_staleness_helper() {
    // timestamp_us = 1_700_000_000_000_000 (some instant T).
    let ts_us = 1_700_000_000_000_000u64;
    let t_secs = ts_us / 1_000_000;
    assert!(!is_stale(ts_us, t_secs, 300));
    assert!(!is_stale(ts_us, t_secs + 300, 300));
    assert!(is_stale(ts_us, t_secs + 301, 300));
    // Clock skew backwards: never stale.
    assert!(!is_stale(ts_us, t_secs - 10, 0));
}

#[test]
fn test_emits_nothing_without_feeds() {
    let (env, _, _, contract_id) = setup();
    let client = StellarFeedContractClient::new(&env, &contract_id);

    // A rejected payload stores nothing and publishes nothing.
    let garbage = Bytes::from_slice(&env, &[0x00u8; 8]);
    let _ = client.try_update_price(&garbage);
    assert!(env.events().all().is_empty());
}
