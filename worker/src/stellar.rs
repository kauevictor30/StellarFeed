//! Submissão on-chain via Stellar CLI (ferramenta oficial, escrita em Rust).
//!
//! ## Por que via CLI e não XDR manual?
//! Montar e assinar um `InvokeHostFunction` na mão exige gerenciar
//! sequence number, footprint de storage, resource fees, simulação e
//! assinatura ed25519 — tudo propenso a erro. O `stellar` CLI (crate
//! oficial `stellar-cli`, mesmo time do `soroban-sdk`) já faz isso e é
//! o caminho recomendado para bots/workers simples.
//!
//! O worker Rust continua sendo o cérebro: ele agenda, busca preços,
//! valida, aplica retry/backoff e registra logs. O CLI é só o
//! "motor de assinatura/envio", invocado como subprocesso.
//!
//! Pré-requisitos na máquina do worker:
//! 1. `stellar` CLI instalado (`cargo install --locked stellar-cli`)
//! 2. Identidade criada e fundada:
//!    `stellar keys generate alice --network testnet --fund`
//! 3. Essa identidade é o `admin` passado no deploy (`-- --admin alice`).
//!
//! ## Migração futura (assinatura 100% in-process)
//! Se quiser eliminar o subprocesso, troque `submit_quote` por chamadas
//! RPC (`getAccount` → `simulateTransaction` → assinar com `ed25519-dalek`
//! + `stellar-strkey` → `sendTransaction`) usando `reqwest`. A interface
//! deste módulo (`submit_quote(&Quote)`) não precisa mudar.

use anyhow::{Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::{info, warn};

use crate::config::Config;
use crate::price::Quote;

/// Envia uma cotação chamando `update_price` no contrato.
///
/// Equivale a:
/// ```sh
/// stellar contract invoke \
///   --id <CONTRACT_ID> --source-account <ADMIN> --network testnet \
///   -- update-price --symbol BRLXLM --price 7200000 --timestamp 1700000000
/// ```
pub async fn submit_quote(cfg: &Config, q: &Quote) -> Result<String> {
    if cfg.dry_run {
        info!(
            symbol = q.symbol,
            price = q.scaled_price,
            "[dry-run] pularia envio on-chain"
        );
        return Ok("dry-run".to_string());
    }

    // Backoff simples: 3 tentativas (2s, 4s, 8s).
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=3 {
        match invoke_once(cfg, q).await {
            Ok(out) => return Ok(out),
            Err(e) => {
                warn!("invoke falhou (tentativa {attempt}/3): {e:#}");
                last_err = Some(e);
                tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt))).await;
            }
        }
    }
    Err(last_err.unwrap())
}

/// Uma tentativa de `stellar contract invoke`.
async fn invoke_once(cfg: &Config, q: &Quote) -> Result<String> {
    let price = q.scaled_price.to_string();
    let timestamp = q.timestamp.to_string();

    let mut cmd = Command::new("stellar");
    cmd.args([
        "contract",
        "invoke",
        "--id",
        &cfg.contract_id,
        "--source-account",
        &cfg.source_account,
        "--network",
        &cfg.network,
        "--",
        "update-price",
        "--symbol",
        q.symbol,
        "--price",
        &price,
        "--timestamp",
        &timestamp,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let output = tokio::time::timeout(Duration::from_secs(60), cmd.output())
        .await
        .context("timeout após 60s esperando o stellar CLI")?
        .context("falha ao executar `stellar` — ele está instalado no PATH?")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info!(symbol = q.symbol, "update_price confirmado: {stdout}");
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("stellar CLI exit {}: {}", output.status, stderr.trim())
    }
}

/// Checagem de saúde na inicialização: CLI presente + RPC alcançável.
pub async fn preflight_check(rpc_url: &str) -> Result<()> {
    // 1. CLI instalado?
    let out = Command::new("stellar")
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .await
        .context("`stellar` CLI não encontrado no PATH")?;
    info!(
        "stellar CLI: {}",
        String::from_utf8_lossy(&out.stdout).trim()
    );

    // 2. RPC responde `getHealth`?
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let res: serde_json::Value = client
        .post(rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "getHealth", "params": []
        }))
        .send()
        .await
        .context("RPC inacessível")?
        .json()
        .await
        .context("resposta do RPC inválida")?;
    info!("RPC health: {res}");
    Ok(())
}

/// URL pública do RPC a partir do nome da rede.
pub fn rpc_url_for(network: &str) -> &'static str {
    match network {
        "mainnet" => "https://mainnet.sorobanrpc.com",
        "futurenet" => "https://rpc-futurenet.stellar.org",
        _ => "https://soroban-testnet.stellar.org",
    }
}
