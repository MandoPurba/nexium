# Nexium

> Cryptocurrency exchange backend in Rust — order matching, wallet management, and real-time market data.

[![CI](https://github.com/MandoPurba/nexium/actions/workflows/ci.yml/badge.svg)](https://github.com/MandoPurba/nexium/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange?logo=rust)
![License](https://img.shields.io/badge/license-MIT-blue)

A microservice-lite exchange backend built as a portfolio + learning project. Independent service binaries share a small set of library crates and a layered configuration model. Communication is HTTP today; NATS and Kafka land in later sprints.

## Status

Active. Solo build over 8 sprints × 2 weeks (~4.5 months, targeting October 2026).

| Sprint | Goal | Status |
|---|---|---|
| 1 | Workspace, infrastructure, migrations, CI | Done |
| 2 | Auth service — register / login / me / API keys | In progress |
| 3 | Wallet — balances, deposit, lock/unlock | |
| 4 | Order placement | |
| 5 | Matching engine — limit orderbook + price-time priority | |
| 6 | Market data — OHLCV, snapshots | |
| 7 | WebSocket realtime | |
| 8 | Hardening — rate limit, OpenTelemetry, metrics | |
| 9 | Polish + deploy | |

## Stack

- **Language:** Rust (edition 2024, 1.85+)
- **Web / async:** Actix-Web 4, tokio, sqlx 0.8
- **Storage:** PostgreSQL 16 (primary), TimescaleDB (market data), Redis 7
- **Crypto:** argon2id (passwords), JWT (auth tokens)
- **Messaging:** NATS JetStream (internal) + Kafka (audit) — Sprint 5+
- **Observability:** OpenTelemetry, Jaeger, Prometheus, Grafana — Sprint 8

Money fields are `NUMERIC(36, 18)` end-to-end, never floats.

## Project layout

```
crates/
  core/             domain types and errors
  config/           layered config loader (TOML + env)
  telemetry/        tracing-subscriber init
  db/               sqlx pool builders
  matching-engine/  pure orderbook + matching logic

services/
  auth/             registration, login, JWT, API keys
  wallet/           balances, deposit, lock/unlock
  order/            place + cancel orders
  market-data/      OHLCV, orderbook snapshots
  gateway/          external HTTP + WebSocket

migrations/
  postgres/         auth + trading + wallet + fee schemas
  timescale/        market hypertables

config/default.toml baseline AppConfig
docker-compose.yml  postgres + timescaledb + redis
Makefile            common dev targets
```

## Getting started

### Prerequisites

- Rust 1.85+ (`rustup install stable`)
- Docker + docker compose
- `sqlx-cli` — `cargo install sqlx-cli --no-default-features --features rustls,postgres`

### Run

```bash
git clone git@github.com:MandoPurba/nexium.git
cd nexium
cp .env.example .env
make up           # start postgres + timescaledb + redis
make migrate      # apply migrations to both databases
cargo run -p auth-service
```

The auth service listens on `0.0.0.0:8080`. Smoke test:

```bash
curl -X POST http://localhost:8080/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"alice@example.com","password":"strongpassword123"}'
```

### Development

```bash
make migrate-info                                       # status per DB
make db-reset                                           # nuke + remigrate
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

CI runs `fmt`, `clippy -D warnings`, and `cargo test --workspace`.

## Configuration

`nexium-config` loads settings in this order (later wins):

1. `config/default.toml`
2. `config/{environment}.toml` — optional per-env overrides
3. Env vars `NEXIUM__SECTION__FIELD` (e.g. `NEXIUM__SERVER__PORT=9090`)
4. Standard vars `DATABASE_URL`, `TIMESCALE_URL`, `REDIS_URL`, `JWT_SECRET` — same `.env` works for the app, docker-compose, sqlx-cli, and the Makefile
5. The `service_name` passed to `AppConfig::load()`

`NEXIUM_ENV` is one of `local` (default), `development`, `production`. Production refuses to start with a weak `auth.jwt_secret`. Secrets are wrapped in `Secret<T>` and masked in `Debug` output.

## API

Every error response uses one envelope:

```json
{ "code": "VALIDATION_ERROR", "message": "...", "details": { ... } }
```

Codes: `VALIDATION_ERROR` (400), `UNAUTHORIZED` (401), `FORBIDDEN` (403), `NOT_FOUND` (404), `CONFLICT` (409), `INSUFFICIENT_BALANCE` (422), `PAIR_INACTIVE` (422), `INTERNAL_ERROR` (500).

Full API surface is implemented sprint-by-sprint per the table above.

## License

MIT
