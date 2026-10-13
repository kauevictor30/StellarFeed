//! # StellarFeed Worker (Rust puro)
//!
//! Daemon off-chain que alimenta o oráculo:
//! ```text
//! CoinGecko (HTTP) → valida/escala → stellar contract invoke (Testnet)
//! ```
//! Loop infinito com intervalo configurável (`POLL_INTERVAL_SECS`).
//! Sem Python: apenas Rust + o CLI oficial `stellar` para assinar/enviar.

mod config;
mod price;
mod stellar;

use std::time::Duration;

use anyhow::Result;
use tracing::{error, info, warn};

use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // Logs estruturados; nível via `RUST_LOG` (default: info).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stellar_feed_worker=info".into()),
        )
        .init();

    let cfg = Config::from_env()?;
    info!(
        network = %cfg.network,
        contract = %cfg.contract_id,
        source = %cfg.source_account,
        interval_s = cfg.poll_interval_secs,
        dry_run = cfg.dry_run,
        "StellarFeed worker iniciando"
    );

    // Falha rápido se o ambiente estiver quebrado (CLI/RPC).
    if !cfg.dry_run {
        if let Err(e) = stellar::preflight_check(stellar::rpc_url_for(&cfg.network)).await {
            warn!("preflight falhou (seguindo mesmo assim): {e:#}");
        }
    }

    let http = reqwest::Client::builder()
        .user_agent("stellar-feed-worker/0.1")
        .timeout(Duration::from_secs(20))
        .build()?;

    // Loop principal: coleta → publica cada símbolo → dorme.
    loop {
        if let Err(e) = run_cycle(&cfg, &http).await {
            error!("ciclo falhou: {e:#}");
        }
        info!("próximo ciclo em {}s", cfg.poll_interval_secs);
        tokio::time::sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}

/// Um ciclo completo: buscar preços e publicar cada símbolo.
async fn run_cycle(cfg: &Config, http: &reqwest::Client) -> Result<()> {
    let quotes = price::fetch_quotes(http, &cfg.coingecko_url, cfg.price_scale).await?;

    for q in &quotes {
        match stellar::submit_quote(cfg, q).await {
            Ok(receipt) => info!(
                symbol = q.symbol,
                human = q.human_price,
                scaled = q.scaled_price,
                "publicado ✓ ({receipt})"
            ),
            Err(e) => error!(symbol = q.symbol, "falha ao publicar: {e:#}"),
        }
        // Pequena pausa entre símbolos p/ não estourar rate-limit do RPC.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Ok(())
}
