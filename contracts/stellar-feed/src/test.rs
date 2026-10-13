//! Testes unitários do StellarFeed.
//! Rode com: `cargo test -p stellar-feed`

use super::*;
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal, Symbol,
};

/// Helper: registra o contrato com um admin gerado.
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
    // NOTE: sem mock_all_auths — vamos autorizar explicitamente como admin
    // e provar que um atacante recebe erro de auth.
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let contract_id = env.register(StellarFeedContract, (admin.clone(),));
    let client = StellarFeedContractClient::new(&env, &contract_id);
    let symbol = Symbol::new(&env, "BRLXLM");

    // Admin de verdade: autoriza e passa.
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

    // Atacante tentando se passar por admin: o host rejeita (não é o admin
    // gravado no storage, então require_auth falha na verificação real).
    // Aqui simulamos a chamada com auth do atacante — o contrato chama
    // `admin.require_auth()`, e como o mock só autoriza `attacker`,
    // o host retorna erro de auth (HostError, não FeedError).
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
    assert!(res.is_err(), "atacante não deveria conseguir atualizar");
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

    // O evento PriceUpdated deve ter sido publicado.
    let events = env.events().all();
    assert!(!events.is_empty(), "esperava ao menos 1 evento");
}
