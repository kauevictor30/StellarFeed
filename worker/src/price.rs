//! Coleta de preços via CoinGecko (API pública, sem chave).
//!
//! Estratégia:
//! - `GET /simple/price?ids=stellar,usd-coin&vs_currencies=brl`
//!   retorna o preço em BRL de 1 XLM e de 1 USDC.
//! - Convertemos para inteiro escalado (`price * PRICE_SCALE`) para
//!   enviar ao contrato (que só trabalha com `i128`).
//!
//! Símbolos on-chain publicados pelo worker:
//! - `"BRLXLM"`  — BRL por 1 XLM
//! - `"BRLUSDC"` — BRL por 1 USDC

use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;
use std::collections::HashMap;
use tracing::{info, warn};

/// Uma cotação pronta para ir ao contrato.
#[derive(Debug, Clone)]
pub struct Quote {
    /// Símbolo on-chain (ex.: `"BRLXLM"`).
    pub symbol: &'static str,
    /// Preço escalado (ex.: R$ 5,12 → `51_200_000` com escala 1e7).
    pub scaled_price: i128,
    /// Preço "humano" (float, só p/ logs).
    pub human_price: f64,
    /// Unix timestamp (segundos) da coleta.
    pub timestamp: u64,
}

/// Resposta da CoinGecko: `{ "stellar": { "brl": 0.72 }, ... }`.
type SimplePrice = HashMap<String, HashMap<String, f64>>;

/// Busca os preços na CoinGecko com retry + backoff exponencial.
pub async fn fetch_quotes(client: &Client, base_url: &str, price_scale: f64) -> Result<Vec<Quote>> {
    let url = format!(
        "{}/simple/price?ids=stellar,usd-coin&vs_currencies=brl",
        base_url.trim_end_matches('/')
    );

    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=4 {
        match client.get(&url).send().await {
            Ok(resp) => {
                if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    warn!("CoinGecko rate-limit (429), tentativa {attempt}/4 — aguardando…");
                    tokio::time::sleep(std::time::Duration::from_secs(5 * attempt as u64)).await;
                    continue;
                }
                let resp = resp
                    .error_for_status()
                    .context("CoinGecko retornou HTTP de erro")?;
                let data: SimplePrice = resp.json().await.context("JSON da CoinGecko inválido")?;
                return build_quotes(&data, price_scale);
            }
            Err(e) => {
                warn!("Falha HTTP (tentativa {attempt}/4): {e:#}");
                last_err = Some(e.into());
                tokio::time::sleep(std::time::Duration::from_secs(2_u64.pow(attempt))).await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("CoinGecko inacessível")))
}

/// Converte a resposta bruta em `Quote`s escaladas.
fn build_quotes(data: &SimplePrice, price_scale: f64) -> Result<Vec<Quote>> {
    // Mapeia id da CoinGecko -> símbolo on-chain.
    const PAIRS: [(&str, &str); 2] = [("stellar", "BRLXLM"), ("usd-coin", "BRLUSDC")];

    let timestamp = Utc::now().timestamp() as u64;
    let mut out = Vec::with_capacity(PAIRS.len());

    for (coin_id, symbol) in PAIRS {
        let human_price = data
            .get(coin_id)
            .and_then(|m| m.get("brl"))
            .copied()
            .with_context(|| format!("CoinGecko não retornou `{coin_id}/brl`"))?;

        if !(human_price.is_finite() && human_price > 0.0) {
            anyhow::bail!("Preço inválido para {coin_id}: {human_price}");
        }

        let scaled_price = (human_price * price_scale).round() as i128;
        info!(symbol, human_price, scaled_price, "cotação coletada");
        out.push(Quote {
            symbol,
            scaled_price,
            human_price,
            timestamp,
        });
    }
    Ok(out)
}

/// Converte float → inteiro escalado (usada em testes e reuso).
pub fn scale_price(human: f64, scale: f64) -> anyhow::Result<i128> {
    if !(human.is_finite() && human > 0.0) {
        anyhow::bail!("preço inválido: {human}");
    }
    Ok((human * scale).round() as i128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_brl_price_to_7_decimals() {
        // R$ 5,1234567 → 51_234_567
        assert_eq!(scale_price(5.1234567, 10_000_000.0).unwrap(), 51_234_567);
    }

    #[test]
    fn rejects_non_positive() {
        assert!(scale_price(0.0, 10_000_000.0).is_err());
        assert!(scale_price(-1.5, 10_000_000.0).is_err());
    }

    #[test]
    fn builds_quotes_from_mock_payload() {
        let mut data: SimplePrice = HashMap::new();
        data.insert("stellar".into(), HashMap::from([("brl".into(), 0.72)]));
        data.insert("usd-coin".into(), HashMap::from([("brl".into(), 5.41)]));

        let quotes = build_quotes(&data, 10_000_000.0).unwrap();
        assert_eq!(quotes.len(), 2);
        assert_eq!(quotes[0].symbol, "BRLXLM");
        assert_eq!(quotes[0].scaled_price, 7_200_000);
        assert_eq!(quotes[1].symbol, "BRLUSDC");
    }
}
