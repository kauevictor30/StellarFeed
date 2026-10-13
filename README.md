# ✦ StellarFeed — Pyth-Backed Price Oracle on Stellar

Trust-minimized price feed, 100% open source: **Rust (Soroban contract + relay worker) + React (dashboard)**.
Zero Python — not a single line.

```text
Pyth Pro websocket (signed leEcdsa) ──▶ relay worker (Rust) ──payload──▶ Soroban ──RPC──▶ React
                                           (no price authority)      │ verifies via
                                                                     pyth-lazer-stellar
```

## Why Pyth instead of a plain API fetch?

The v1 design had the worker fetch CoinGecko over HTTP and push prices with an admin key — that makes **us** the oracle: a single centralized source with no cryptographic provenance. Anyone must trust our server.

In the current design the worker only forwards **opaque Pyth-signed bytes**. The contract cross-calls the official `pyth-lazer-stellar` verifier and stores a price **only if the signature checks out**. Relaying is permissionless and failure-closed: a compromised worker can withhold updates (liveness) but never invent prices (safety). This is the pull-oracle model from [Pyth's Stellar guide](https://docs.pyth.network/price-feeds/pro/integrate-as-consumer/stellar).

| Module | Folder | Stack |
|---|---|---|
| On-chain consumer | `contracts/stellar-feed` | Rust + `soroban-sdk` v26 + `pyth-lazer-stellar-sdk` v0.4 (`#![no_std]`, WASM) |
| Relay worker | `worker` | Rust + `tokio` + `tokio-tungstenite` + Stellar CLI |
| Frontend | `frontend` | React + Vite + `@stellar/stellar-sdk` + `@stellar/freighter-api` |

> **SDK pin:** `pyth-lazer-stellar-sdk 0.4` requires `soroban-sdk ^26.1.0` — the contract crate is pinned to v26 on purpose. Do not bump it independently.

> **RedStone/SEP-40 note:** RedStone ships SEP-40 feeds on Stellar (XLM, USDC, …). Our read API (`get_price` / `get_price_checked`) is intentionally narrow so a SEP-40 façade can be added later without breaking consumers.

## 1. Prerequisites

| Tool | Minimum version | Install |
|---|---|---|
| Rust | 1.84+ | `curl https://sh.rustup.rs -sSf \| sh` |
| Target WASM | — | `rustup target add wasm32v1-none` |
| Stellar CLI | recent | `cargo install --locked stellar-cli` |
| Node.js | 22+ | `nvm install 22` (SDK v16 requires Node 22+) |
| Pyth Pro API key | — | [Acquire an API key](https://docs.pyth.network/price-feeds/pro/subscribe-to-prices) (server-side only, never commit it) |

Deployed `pyth-lazer-stellar` verifiers (pass as `--lazer` at deploy):

| Network | Verifier contract |
|---|---|
| Testnet | `CAYFT5JE3UQTKT4Q6ZOZK4FXVYVT6RE3MFC7STA4UB6WAEGBT65MRU52` |
| Mainnet | `CACZ3GBAKUPIAFRILUFO27J5RUH5GJ2VSJ46LP6GJYSKGDRTQ5MS3HCH` |

## 2. Contract (Soroban / Rust, Pyth consumer)

```bash
# Unit tests (no network needed; success paths of update_price need a
# genuinely signed payload — covered by fork tests, see backlog)
cargo test -p stellar-feed

# Optimized WASM build (do NOT use `cargo build` for deploys)
stellar contract build
# → target/wasm32v1-none/release/stellar_feed.wasm

# Relayer identity (pays fees only — holds NO price authority)
stellar keys generate alice --network testnet --fund

# Deploy (--lazer is the verifier above, --admin only rotates config)
stellar contract deploy \
  --wasm target/wasm32v1-none/release/stellar_feed.wasm \
  --source-account alice \
  --network testnet \
  -- --lazer CAYFT5JE3UQTKT4Q6ZOZK4FXVYVT6RE3MFC7STA4UB6WAEGBT65MRU52 \
     --admin alice
# → note the contract ID (CABC...)
```

### Contract API

| Function | Auth | Description |
|---|---|---|
| `__constructor(lazer, admin)` | deploy | Sets verifier + config admin — runs once |
| `update_price(payload)` | none (anyone may relay) | Verifies `leEcdsa` bytes via the verifier, stores every fully-populated feed; garbage fails closed with `VerifyFailed` |
| `get_price(feed_id)` | none (read-only) | Returns `StoredPrice { price, exponent, timestamp_us }` or `NoPriceData` |
| `get_price_checked(feed_id, max_age_secs)` | none (read-only) | Same, plus `StalePrice` when older than `max_age_secs` (ledger clock) |
| `set_lazer(...)` / `set_admin(...)` | Admin only | Config rotation |
| `get_lazer()` / `get_admin()` / `version()` | none | Metadata |

Prices are keyed by Pyth **feed ID** (`u32`; BTC/USD = 1 — full list at the [Price Feed IDs](https://docs.pyth.network/price-feeds/pro/price-feed-ids) page). Real value = `price * 10^exponent`. Every stored feed publishes a `PriceUpdated` event and extends its TTL (~90 days).

## 3. Relay worker (pure Rust)

Streams signed envelopes from Pyth Pro and relays the latest one every `RELAY_INTERVAL_SECS` (chain writes cost fees — ticks are not relayed 1:1).

```bash
cd worker
cp .env.example .env
# edit: STELLAR_CONTRACT_ID=<deploy output>, PYTH_API_KEY=<your key>,
#       PYTH_FEED_IDS=<ids from the feed list>

# Receive-and-log only (no fees spent) — recommended for the first run:
DRY_RUN=1 cargo run -p stellar-feed-worker

# Production (testnet):
cargo run --release -p stellar-feed-worker
```

> Connect a second worker with `PYTH_WS_URL=wss://pyth-lazer-1.dourolabs.app/v1/stream` for endpoint redundancy, as Pyth recommends.

## 4. Frontend (React + Vite + Freighter)

Read-only dashboard: reads `get_price(feed_id)` via RPC (`simulateTransaction`, free, no wallet needed) and displays it live with polling. Configure visible feeds with `VITE_FEEDS="id:Label,..."`.

```bash
cd frontend
npm install
cp .env.example .env
# edit: VITE_CONTRACT_ID=<your contract ID>, VITE_FEEDS=<ids:labels>

npm run dev      # http://localhost:5173
npm run build    # dist/ ready for static hosting
```

## 5. Consume StellarFeed from your contract (SDK crate)

Third-party contracts don't need to hand-roll cross-contract calls — depend on the published client crate:

```toml
# Cargo.toml of YOUR contract (soroban-sdk major must match: 26)
stellar-feed-sdk = "0.1"
```

```rust
use stellar_feed_sdk::StellarFeedClient;

// BTC/USD (feed 1), at most 5 minutes old:
let quote = StellarFeedClient::new(&env, &oracle)
    .get_price_checked(1, 300)
    .expect("stale");
// Real value = quote.price * 10^quote.exponent.
```

See `contracts/demo-consumer` for a minimal working integration (mock-oracle tests included).

### Publishing the SDK to crates.io (maintainers)

```bash
cargo login                              # one-time, paste your crates.io token
cargo publish -p stellar-feed-sdk        # name `stellar-feed-sdk` is free
```

Rules: bump `version` every publish, never republish the same version, and keep `soroban-sdk` on the same major the oracle uses.

## 6. Layout

```text
StellarFeed/
├── Cargo.toml                    # workspace (contracts + worker)
├── contracts/stellar-feed/
│   ├── Cargo.toml                # soroban-sdk v26 + pyth-lazer-stellar-sdk 0.4
│   └── src/{lib.rs,test.rs}      # Pyth consumer + unit tests
├── contracts/stellar-feed-sdk/
│   └── src/lib.rs                # publishable typed client (no_std)
├── contracts/demo-consumer/
│   └── src/{lib.rs,test.rs}      # minimal integration example
├── worker/
│   ├── Cargo.toml                # tokio, tokio-tungstenite, reqwest, serde, anyhow, tracing
│   └── src/{main.rs,config.rs,lazer.rs,stellar.rs}
├── frontend/
│   ├── package.json              # react, stellar-sdk, freighter-api, vite
│   └── src/{App.jsx,main.jsx,stellar.js,hooks/useFreighter.js,
│             components/{PriceCard.jsx,ConnectButton.jsx}}
└── README.md
```

## 7. Security

- Trust boundary is `verify_update` inside the contract — never trust caller-parsed data (Pyth's rule).
- `update_price` is permissionless by design; admin keys cannot forge prices.
- Freshness enforced on-chain (`get_price_checked` vs ledger clock); TTL extended on every update against archival.
- Secrets never committed (`.env` in `.gitignore`; API key server-side only — never in the frontend).
- Before mainnet: full test suite + fork tests with recorded payloads, Soroban security checklist, 7-day testnet soak, mainnet RPC provider, incident plan.

## 8. License

Apache-2.0. PRs are welcome.
