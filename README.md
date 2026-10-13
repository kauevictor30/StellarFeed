# ✦ StellarFeed — Stellar-Native Price Oracle

High-performance, 100% open source price feed: **Rust (Soroban contract + worker) + React (dashboard)**.
Zero Python — not a single line.

```text
CoinGecko ──HTTP──▶ worker (Rust) ──stellar contract invoke──▶ Soroban (Rust) ──RPC──▶ React
                           │                              │                          │
                      every 5 min                    BRLXLM / BRLUSDC            Freighter + live
```

| Module | Folder | Stack |
|---|---|---|
| On-chain contract | `contracts/stellar-feed` | Rust + `soroban-sdk` v27 (`#![no_std]`, WASM) |
| Off-chain worker | `worker` | Rust + `tokio` + `reqwest` + official Stellar CLI |
| Frontend | `frontend` | React + Vite + `@stellar/stellar-sdk` + `@stellar/freighter-api` |

## 1. Prerequisites

| Tool | Minimum version | Install |
|---|---|---|
| Rust | 1.84+ | `curl https://sh.rustup.rs -sSf \| sh` |
| WASM target | — | `rustup target add wasm32v1-none` |
| Stellar CLI | recent | `cargo install --locked stellar-cli` |
| Node.js | 22+ | `nvm install 22` (SDK v16 requires Node 22+) |

## 2. Contract (Soroban / Rust)

```bash
# Unit tests (no network needed)
cargo test -p stellar-feed

# Optimized WASM build (do NOT use `cargo build` for deploys)
stellar contract build
# → target/wasm32v1-none/release/stellar_feed.wasm

# Admin identity (the Worker) + testnet funding
stellar keys generate alice --network testnet --fund

# Deploy (--admin is the Worker address)
stellar contract deploy \
  --wasm target/wasm32v1-none/release/stellar_feed.wasm \
  --source-account alice \
  --network testnet \
  -- --admin alice
# → note the contract ID (CABC...)

# Test read (no signature required)
stellar contract invoke \
  --id <CONTRACT_ID> --source-account alice --network testnet \
  -- get-price --symbol BRLXLM
```

### Contract API

| Function | Auth | Description |
|---|---|---|
| `__constructor(admin)` | deploy | Sets the Admin (Worker) — runs once |
| `update_price(symbol, price, timestamp)` | Admin only | `price: i128` scaled by 1e7, `timestamp: u64` unix |
| `get_price(symbol)` | none (read-only) | Returns `PriceData { price, timestamp }` or `NoPriceData` |
| `set_admin(new_admin)` | Admin only | Rotates the Worker |
| `get_admin()` / `version()` | none | Metadata |

Convention: `R$ 5.1234567 → 51_234_567`. Symbols: `"BRLXLM"`, `"BRLUSDC"` (max 32 chars).
Every `update_price` publishes a `PriceUpdated` event (indexable via RPC `getEvents`) and extends the entry TTL (~90 days).

## 3. Worker (pure Rust)

Daemon that every `POLL_INTERVAL_SECS` fetches `stellar` + `usd-coin` in BRL from CoinGecko and invokes `update-price` per symbol, with retry/backoff and `tracing` logs.

```bash
cd worker
cp .env.example .env
# edit: STELLAR_CONTRACT_ID=<deploy output>, STELLAR_SOURCE_ACCOUNT=alice

# Fetch-and-log only (no fees spent) — recommended for the first run:
DRY_RUN=1 cargo run -p stellar-feed-worker

# Production (testnet):
cargo run --release -p stellar-feed-worker
# or: RUST_LOG=info POLL_INTERVAL_SECS=300 ./target/release/stellar-feed-worker
```

> Why does the worker shell out to the `stellar` CLI instead of signing XDR by hand? Building an `InvokeHostFunction` manually requires sequence numbers, storage footprints, resource fees, and ed25519 signing — all error-prone. The official CLI (`stellar-cli` crate, same team as `soroban-sdk`) already handles all of that. Rust stays the brain (scheduling, validation, retries, logs); the CLI is just the sign-and-send engine. `submit_quote()` in `worker/src/stellar.rs` isolates that layer — it can be swapped for in-process signing (`ed25519-dalek` + RPC `simulate/sendTransaction`) without touching the rest.

## 4. Frontend (React + Vite + Freighter)

Read-only dashboard: reads `get_price` via RPC (`simulateTransaction`, free, no wallet needed) and displays it live with polling. The Freighter wallet identifies the user.

```bash
cd frontend
npm install
cp .env.example .env
# edit: VITE_CONTRACT_ID=<your contract ID>

npm run dev      # http://localhost:5173
npm run build    # dist/ ready for static hosting
```

## 5. Layout

```text
StellarFeed/
├── Cargo.toml                    # workspace (contracts + worker)
├── contracts/stellar-feed/
│   ├── Cargo.toml                # soroban-sdk v27, crate-type cdylib
│   └── src/{lib.rs,test.rs}      # oracle + 8 unit tests
├── worker/
│   ├── Cargo.toml                # tokio, reqwest, serde, anyhow, tracing, chrono
│   └── src/{main.rs,config.rs,price.rs,stellar.rs}
├── frontend/
│   ├── package.json              # react, stellar-sdk, freighter-api, vite
│   └── src/{App.jsx,main.jsx,stellar.js,hooks/useFreighter.js,
│             components/{PriceCard.jsx,ConnectButton.jsx}}
└── README.md
```

## 6. Security

- `update_price` requires `admin.require_auth()` — nobody but the Worker can write.
- On-chain validation (`price > 0`, `timestamp > 0`); TTL extended on every update against archival.
- Secrets never committed (`.env` in `.gitignore`); the frontend never receives a private key.
- Before mainnet: run `cargo test`, review the Soroban security checklist (auth, TTL, arithmetic), and deploy to testnet first.

## 7. License

Apache-2.0. PRs are welcome.
