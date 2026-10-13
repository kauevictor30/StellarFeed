#![no_std]
//! # StellarFeed — Oráculo de Preços (Soroban / Rust)
//!
//! Contrato minimalista e auditável que armazena a cotação mais recente
//! de cada ativo (ex: `"BRLXLM"`, `"BRLUSDC"`).
//!
//! ## Convenção de preço
//! - `price` é um `i128` com **7 casas decimais** (fator `10_000_000`),
//!   o mesmo padrão de precisão da Stellar.
//!   Ex.: R$ 5,1234567 → `51_234_567`.
//! - `timestamp` é Unix seconds (ledger timestamp validado off-chain).
//!
//! ## Autorização
//! - `update_price` exige `admin.require_auth()` — só o Worker off-chain.
//! - `get_price` é read-only, qualquer dApp pode ler sem assinatura.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env, Symbol,
};

/// Fator de escala fixo: 10^7.
pub const PRICE_SCALE: i128 = 10_000_000;

/// Versão do contrato (para o frontend exibir / checar upgrades).
pub const VERSION: u32 = 1;

/// Chaves de storage.
///
/// - `Admin` fica no **instance storage** (único por contrato).
/// - Cada preço fica no **persistent storage** (um entry por símbolo,
///   com TTL próprio estendido a cada update).
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Price(Symbol),
}

/// Cotação armazenada.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    /// Preço escalado por PRICE_SCALE.
    pub price: i128,
    /// Unix timestamp (segundos) informado pelo Worker.
    pub timestamp: u64,
}

/// Erros tipados do contrato.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FeedError {
    /// `__constructor` ainda não rodou / Admin não setado.
    NotInitialized = 1,
    /// Preço precisa ser > 0.
    InvalidPrice = 2,
    /// Timestamp zero (nunca deve acontecer; protege contra default).
    InvalidTimestamp = 3,
    /// Nenhuma cotação registrada para este símbolo ainda.
    NoPriceData = 4,
}

/// Evento publicado a cada atualização — indexável via RPC `getEvents`.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceUpdated {
    /// Símbolo atualizado (ex: "BRLXLM"). Vira tópico do evento.
    #[topic]
    pub symbol: Symbol,
    /// Preço escalado.
    pub price: i128,
    /// Timestamp informado pelo Worker.
    pub timestamp: u64,
}

#[contract]
pub struct StellarFeedContract;

#[contractimpl]
impl StellarFeedContract {
    // ------------------------------------------------------------------
    // Inicialização (roda 1x, atomicamente, no deploy)
    // ------------------------------------------------------------------

    /// Inicializa o contrato definindo o Admin (endereço do Worker).
    ///
    /// Chamado via: `stellar contract deploy ... -- --admin <ADDRESS>`
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        // TTL generoso p/ o instance entry (~180 dias em ledgers de 5s).
        env.storage().instance().extend_ttl(100_000, 1_000_000);
    }

    // ------------------------------------------------------------------
    // Escrita — apenas Admin (Worker)
    // ------------------------------------------------------------------

    /// Registra/atualiza a cotação de um símbolo.
    ///
    /// # Argumentos
    /// - `symbol`: ex. `Symbol::new(&env, "BRLXLM")` (máx. 32 chars).
    /// - `price`: valor escalado por 1e7. Deve ser > 0.
    /// - `timestamp`: Unix seconds. Deve ser > 0.
    ///
    /// # Autorização
    /// Exige assinatura do Admin (`admin.require_auth()`).
    pub fn update_price(
        env: Env,
        symbol: Symbol,
        price: i128,
        timestamp: u64,
    ) -> Result<(), FeedError> {
        // 1. Autorização: só o Admin.
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(FeedError::NotInitialized)?;
        admin.require_auth();

        // 2. Validação de entrada (barata, antes de escrever).
        if price <= 0 {
            return Err(FeedError::InvalidPrice);
        }
        if timestamp == 0 {
            return Err(FeedError::InvalidTimestamp);
        }

        // 3. Escrita no persistent storage + extensão de TTL.
        //    (~30 dias threshold / ~90 dias extend-to, em ledgers de ~5s:
        //     17280 ledgers ≈ 1 dia.)
        let key = DataKey::Price(symbol.clone());
        let data = PriceData { price, timestamp };
        env.storage().persistent().set(&key, &data);
        env.storage()
            .persistent()
            .extend_ttl(&key, 30 * 17280, 90 * 17280);

        // 4. Evento para indexadores/dashboards.
        PriceUpdated {
            symbol,
            price,
            timestamp,
        }
        .publish(&env);

        Ok(())
    }

    /// Rotaciona o Admin. Apenas o Admin atual pode chamar.
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
    // Leitura — pública, sem autenticação (usada por dApps e frontend)
    // ------------------------------------------------------------------

    /// Retorna a cotação mais recente de um símbolo.
    /// Falha com `NoPriceData` se nunca foi publicado.
    pub fn get_price(env: Env, symbol: Symbol) -> Result<PriceData, FeedError> {
        env.storage()
            .persistent()
            .get(&DataKey::Price(symbol))
            .ok_or(FeedError::NoPriceData)
    }

    /// Retorna o endereço Admin atual.
    pub fn get_admin(env: Env) -> Result<Address, FeedError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(FeedError::NotInitialized)
    }

    /// Versão do contrato (útil p/ compatibilidade do frontend/worker).
    pub fn version() -> u32 {
        VERSION
    }
}

// ----------------------------------------------------------------------
// Testes unitários (rodam com `cargo test`, sem precisar de rede)
// ----------------------------------------------------------------------
#[cfg(test)]
mod test;
