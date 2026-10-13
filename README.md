# ✦ StellarFeed — Oráculo de Preços nativo da Stellar

Price feed de alta performance e 100% open source: **Rust (contrato Soroban + worker) + React (dashboard)**.
Zero Python — nenhuma linha.

```text
CoinGecko ──HTTP──▶ worker (Rust) ──stellar contract invoke──▶ Soroban (Rust) ──RPC──▶ React
                           │                              │                          │
                      a cada 5 min                   BRLXLM / BRLUSDC            Freighter + live
```

| Módulo | Pasta | Stack |
|---|---|---|
| Contrato on-chain | `contracts/stellar-feed` | Rust + `soroban-sdk` v27 (`#![no_std]`, WASM) |
| Worker off-chain | `worker` | Rust + `tokio` + `reqwest` + Stellar CLI oficial |
| Frontend | `frontend` | React + Vite + `@stellar/stellar-sdk` + `@stellar/freighter-api` |

## 1. Pré-requisitos

| Ferramenta | Versão mínima | Instalação |
|---|---|---|
| Rust | 1.84+ | `curl https://sh.rustup.rs -sSf \| sh` |
| Target WASM | — | `rustup target add wasm32v1-none` |
| Stellar CLI | recente | `cargo install --locked stellar-cli` |
| Node.js | 22+ | `nvm install 22` (SDK v16 exige Node 22+) |

## 2. Contrato (Soroban / Rust)

```bash
# Testes unitários (sem rede)
cargo test -p stellar-feed

# Build do WASM otimizado (NÃO use `cargo build` p/ deploy)
stellar contract build
# → target/wasm32v1-none/release/stellar_feed.wasm

# Identidade admin (o Worker) + funding na testnet
stellar keys generate alice --network testnet --fund

# Deploy (o --admin é o endereço do Worker)
stellar contract deploy \
  --wasm target/wasm32v1-none/release/stellar_feed.wasm \
  --source-account alice \
  --network testnet \
  -- --admin alice
# → anote o Contract ID (CABC...)

# Leitura de teste (sem assinatura)
stellar contract invoke \
  --id <CONTRACT_ID> --source-account alice --network testnet \
  -- get-price --symbol BRLXLM
```

### API do contrato

| Função | Auth | Descrição |
|---|---|---|
| `__constructor(admin)` | deploy | Define o Admin (Worker) — roda 1x |
| `update_price(symbol, price, timestamp)` | só Admin | `price: i128` escalado por 1e7, `timestamp: u64` unix |
| `get_price(symbol)` | ninguém (read-only) | Retorna `PriceData { price, timestamp }` ou `NoPriceData` |
| `set_admin(new_admin)` | só Admin | Rotaciona o Worker |
| `get_admin()` / `version()` | ninguém | Metadados |

Convenção: `R$ 5,1234567 → 51_234_567`. Símbolos: `"BRLXLM"`, `"BRLUSDC"` (máx. 32 chars).
Cada `update_price` publica o evento `PriceUpdated` (indexável via RPC `getEvents`) e estende o TTL do entry (~90 dias).

## 3. Worker (Rust puro)

Daemon que a cada `POLL_INTERVAL_SECS` busca `stellar` + `usd-coin` em BRL na CoinGecko e invoca `update-price` para cada símbolo, com retry/backoff e logs `tracing`.

```bash
cd worker
cp .env.example .env
# edite: STELLAR_CONTRACT_ID=<saída do deploy>, STELLAR_SOURCE_ACCOUNT=alice

# Só coleta e loga (sem gastar taxa) — recomendado no 1º teste:
DRY_RUN=1 cargo run -p stellar-feed-worker

# Produção (testnet):
cargo run --release -p stellar-feed-worker
# ou: RUST_LOG=info POLL_INTERVAL_SECS=300 ./target/release/stellar-feed-worker
```

> Por que o worker chama o `stellar` CLI em vez de assinar XDR na mão? Montar `InvokeHostFunction` manualmente exige sequence number, footprint, resource fees e assinatura ed25519 — o CLI oficial (crate `stellar-cli`, mesmo time do `soroban-sdk`) já faz tudo isso. O Rust continua sendo o cérebro (agenda, valida, retry, logs); o CLI é só o motor de assinatura. A função `submit_quote()` em `worker/src/stellar.rs` isola essa camada — dá para trocar por assinatura in-process (`ed25519-dalek` + RPC `simulate/sendTransaction`) sem mudar o resto.

## 4. Frontend (React + Vite + Freighter)

Dashboard read-only: lê `get_price` via RPC (`simulateTransaction`, grátis, sem carteira) e exibe em tempo real com polling. A carteira Freighter identifica o usuário.

```bash
cd frontend
npm install
cp .env.example .env
# edite: VITE_CONTRACT_ID=<seu Contract ID>

npm run dev      # http://localhost:5173
npm run build    # dist/ pronta p/ deploy estático
```

## 5. Estrutura

```text
StellarFeed/
├── Cargo.toml                    # workspace (contracts + worker)
├── contracts/stellar-feed/
│   ├── Cargo.toml                # soroban-sdk v27, crate-type cdylib
│   └── src/{lib.rs,test.rs}      # oráculo + 8 testes unitários
├── worker/
│   ├── Cargo.toml                # tokio, reqwest, serde, anyhow, tracing, chrono
│   └── src/{main.rs,config.rs,price.rs,stellar.rs}
├── frontend/
│   ├── package.json              # react, stellar-sdk, freighter-api, vite
│   └── src/{App.jsx,main.jsx,stellar.js,hooks/useFreighter.js,
│             components/{PriceCard.jsx,ConnectButton.jsx}}
└── README.md
```

## 6. Segurança

- `update_price` exige `admin.require_auth()` — ninguém além do Worker escreve.
- Validação on-chain (`price > 0`, `timestamp > 0`); TTL estendido a cada update anti-arquivamento.
- Segredos nunca commitados (`.env` no `.gitignore`); frontend nunca recebe chave privada.
- Antes da mainnet: rode `cargo test`, revise `security.md` do checklist Soroban (auth, TTL, aritmética) e faça deploy primeiro na testnet.

## 7. Licença

Apache-2.0. Contribuições via PR são bem-vindas.
