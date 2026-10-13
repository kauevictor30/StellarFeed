//! Configuração do Worker via variáveis de ambiente.
//!
//! Copie `worker/.env.example` para `worker/.env` (ou exporte as vars)
//! e ajuste para o seu ambiente.

use std::env;

use anyhow::{Context, Result};

/// Config imutável do worker, resolvida uma vez na inicialização.
#[derive(Debug, Clone)]
pub struct Config {
    /// Contract ID do StellarFeed na Testnet (ex.: `CABC...`).
    pub contract_id: String,
    /// Identidade-fonte cadastrada no `stellar keys` (é o Admin on-chain).
    pub source_account: String,
    /// Rede Stellar: `testnet` (default) ou `mainnet`.
    pub network: String,
    /// URL base do CoinGecko (permite mock em testes).
    pub coingecko_url: String,
    /// Intervalo entre ciclos de atualização, em segundos.
    pub poll_interval_secs: u64,
    /// Fator de escala do preço (deve bater com `PRICE_SCALE` do contrato).
    pub price_scale: f64,
    /// Se `true`, só busca e loga — não envia transação.
    pub dry_run: bool,
}

impl Config {
    /// Lê as variáveis de ambiente (com defaults sensatos).
    pub fn from_env() -> Result<Self> {
        // Carrega `.env` se existir; ignora erro (vars podem vir do shell/CI).
        let _ = dotenvy::dotenv();

        let contract_id = env::var("STELLAR_CONTRACT_ID").context(
            "STELLAR_CONTRACT_ID não definido. Faça deploy do contrato e exporte o Contract ID.",
        )?;
        let source_account = env::var("STELLAR_SOURCE_ACCOUNT").unwrap_or_else(|_| "alice".into());
        let network = env::var("STELLAR_NETWORK").unwrap_or_else(|_| "testnet".into());
        let coingecko_url =
            env::var("COINGECKO_URL").unwrap_or_else(|_| "https://api.coingecko.com/api/v3".into());
        let poll_interval_secs = env::var("POLL_INTERVAL_SECS")
            .unwrap_or_else(|_| "300".into())
            .parse::<u64>()
            .context("POLL_INTERVAL_SECS inválido (esperado inteiro)")?;
        let price_scale = env::var("PRICE_SCALE")
            .unwrap_or_else(|_| "10000000".into())
            .parse::<f64>()
            .context("PRICE_SCALE inválido")?;
        let dry_run = env::var("DRY_RUN")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        Ok(Self {
            contract_id,
            source_account,
            network,
            coingecko_url,
            poll_interval_secs,
            price_scale,
            dry_run,
        })
    }
}
