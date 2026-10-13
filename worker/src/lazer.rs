//! Pyth Pro websocket subscriber (signed `leEcdsa` payloads).
//!
//! Flow (see https://docs.pyth.network/price-feeds/pro/subscribe-to-prices):
//! 1. Open `wss://...` with `Authorization: Bearer <PYTH_API_KEY>`.
//! 2. Send `{type: subscribe, priceFeedIds, properties, formats: [leEcdsa], ...}`.
//! 3. Every tick carries a signed binary payload; the latest one is shared
//!    with the relay loop through a `watch` channel.
//!
//! The worker never interprets prices — it forwards opaque signed bytes.
//! Trust lives in the on-chain verifier, not here. Reconnects forever
//! with backoff; Pyth recommends connecting to all three endpoints for
//! redundancy (configure a second worker with `PYTH_WS_URL` pointing at
//! `pyth-lazer-1` / `pyth-lazer-2`).

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{info, warn};

use crate::config::Config;

/// Latest signed payload (hex) plus arrival time.
#[derive(Debug, Clone)]
pub struct SignedPayload {
    /// Hex-encoded `leEcdsa` envelope, ready for `update-price --payload`.
    pub hex: String,
    /// When it arrived (used for freshness logs).
    pub received_at: Instant,
}

/// Runs forever: connect → subscribe → forward payloads → reconnect on drop.
pub async fn run_subscriber(cfg: Config, tx: watch::Sender<Option<SignedPayload>>) {
    let mut backoff_secs = 1u64;
    loop {
        match subscribe_once(&cfg, &tx).await {
            Ok(()) => {
                // Clean server-side close: reconnect immediately-ish.
                warn!("websocket closed by server — reconnecting");
                backoff_secs = 1;
            }
            Err(e) => {
                warn!("websocket error: {e:#} — retrying in {backoff_secs}s");
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(60);
            }
        }
    }
}

/// One connection lifetime: handshake, subscribe, forward until it breaks.
async fn subscribe_once(
    cfg: &Config,
    tx: &watch::Sender<Option<SignedPayload>>,
) -> anyhow::Result<()> {
    use anyhow::Context;

    let request = http::Request::builder()
        .uri(&cfg.pyth_ws_url)
        .header("Authorization", format!("Bearer {}", cfg.pyth_api_key))
        .body(())
        .context("invalid websocket URL")?;
    let (stream, _) = connect_async(request)
        .await
        .context("websocket handshake failed")?;
    info!(url = %cfg.pyth_ws_url, "connected to Pyth Pro");
    let (mut write, mut read) = stream.split();

    // `leEcdsa` is the only binary format Soroban can verify (host-side
    // secp256k1). `feedUpdateTimestamp` lets consumers check freshness.
    let subscribe = json!({
        "type": "subscribe",
        "subscriptionId": 1,
        "priceFeedIds": cfg.pyth_feed_ids,
        "properties": ["price", "feedUpdateTimestamp"],
        "formats": ["leEcdsa"],
        "channel": cfg.pyth_channel,
        "ignoreInvalidFeeds": true,
        "jsonBinaryEncoding": "hex",
    });
    write
        .send(Message::Text(subscribe.to_string().into()))
        .await
        .context("subscribe send failed")?;
    info!(feeds = ?cfg.pyth_feed_ids, channel = %cfg.pyth_channel, "subscribed");

    let mut count = 0u64;
    while let Some(msg) = read.next().await {
        let msg = msg.context("websocket read failed")?;
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => anyhow::bail!("server closed the connection"),
            // Ping/pong/binary frames: ignore.
            _ => continue,
        };
        if let Some(hex) = extract_payload_hex(&text) {
            count += 1;
            if count % 50 == 1 {
                info!(bytes = hex.len() / 2, "signed payload received (#{count})");
            }
            let _ = tx.send(Some(SignedPayload {
                hex,
                received_at: Instant::now(),
            }));
        }
    }
    anyhow::bail!("websocket stream ended")
}

/// Pulls the signed envelope hex out of a `streamUpdated` message.
///
/// The binary payload arrives under the key matching the requested format
/// (`leEcdsa`), as `{encoding: "hex", data: "<hex>"}`. A couple of aliases
/// are accepted defensively in case the server echoes another format.
fn extract_payload_hex(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    for key in ["leEcdsa", "solana", "evm"] {
        if let Some(data) = v.get(key)?.get("data").and_then(Value::as_str) {
            if !data.is_empty() {
                return Some(data.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_leecdsa_hex() {
        let msg = r#"{"type":"streamUpdated","subscriptionId":1,
            "leEcdsa":{"encoding":"hex","data":"b901ab12"}}"#;
        assert_eq!(extract_payload_hex(msg).as_deref(), Some("b901ab12"));
    }

    #[test]
    fn ignores_messages_without_payload() {
        assert!(extract_payload_hex(r#"{"type":"subscribed"}"#).is_none());
        assert!(extract_payload_hex("not json").is_none());
    }
}
