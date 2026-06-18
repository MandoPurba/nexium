# Nexium

> Cryptocurrency exchange backend in Rust — order matching, wallet management, real-time market data, and observability.

[![CI](https://github.com/MandoPurba/nexium/actions/workflows/ci.yml/badge.svg)](https://github.com/MandoPurba/nexium/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange?logo=rust)

A microservice-lite exchange backend built as a portfolio + learning project. Independent service binaries share a small set of library crates and communicate via HTTP, NATS, and an in-process matching engine. Designed for correctness (never floats for money), observability (OpenTelemetry + Prometheus), and clarity.

## Architecture

```
┌────────────┐     ┌─────────────┐     ┌──────────────┐
│  Clients   │────▸│   Gateway   │◂───▸│     NATS     │
│ (REST/WS)  │     │  (WS only)  │     │  (pub/sub)   │
└────────────┘     └─────────────┘     └──────┬───────┘
                                              │
       ┌──────────────────────────────────────┼──────────────────┐
       │                                      │                  │
┌──────▼──────┐  ┌──────────────┐  ┌─────────▼────┐  ┌──────────▼───────┐
│ Auth Service│  │Wallet Service│  │Order Service  │  │Market Data Service│
│ :8080       │  │ :8081        │  │ :8082         │  │ :8083             │
└──────┬──────┘  └──────┬───────┘  └──────┬────────┘  └────────┬─────────┘
       │                │                  │                     │
       └────────────────┴──────────────────┴─────────────────────┘
                               │                    │
                        ┌──────▼──────┐     ┌───────▼───────┐
                        │ PostgreSQL  │     │  TimescaleDB  │
                        │ (primary)   │     │ (time-series) │
                        └─────────────┘     └───────────────┘
```

### Services

| Service | Port | Description |
|---------|------|-------------|
| **auth-service** | 8080 | Registration, login (JWT), `/me`, API key management |
| **wallet-service** | 8081 | Multi-currency wallets, deposits, balance lock/unlock |
| **order-service** | 8082 | Order placement/cancellation, matching engine, settlement, fees |
| **market-data-service** | 8083 | OHLCV candles, orderbook snapshots, trade history |
| **gateway** | 8084 | WebSocket real-time feeds (trades, orderbook, user orders) |

### Shared Crates

| Crate | Purpose |
|-------|---------|
| `nexium-core` | Domain types, error envelope, JWT, middleware, rate limiting, metrics |
| `nexium-config` | Layered config loader (TOML + env + `.env`) |
| `nexium-telemetry` | Tracing + OpenTelemetry OTLP setup |
| `nexium-db` | PostgreSQL and TimescaleDB pool builders |
| `nexium-matching-engine` | Pure orderbook + price-time priority matching (no I/O) |

## Tech Stack

- **Language:** Rust (edition 2024, 1.85+)
- **Web framework:** Actix-Web 4
- **Database:** PostgreSQL 16 (primary), TimescaleDB (market data), Redis 7 (cache)
- **Messaging:** NATS (real-time pub/sub for WebSocket feeds)
- **Auth:** Argon2id (passwords), JWT (access tokens), SHA-256 (API keys)
- **Observability:** OpenTelemetry → Jaeger (traces), Prometheus (metrics)
- **API docs:** utoipa + Swagger UI (per-service at `/docs/`)
- **Financials:** `NUMERIC(36, 18)` / `rust_decimal::Decimal` end-to-end — never floats

## Getting Started

### Prerequisites

- Rust 1.85+ (`rustup install stable`)
- Docker + docker compose
- `sqlx-cli` — `cargo install sqlx-cli --no-default-features --features rustls,postgres`

### Quick Start

```bash
git clone git@github.com:MandoPurba/nexium.git
cd nexium
cp .env.example .env
make up           # start postgres, timescaledb, redis, nats, jaeger
make migrate      # apply migrations to both databases

# Run services (each in its own terminal)
cargo run -p auth-service
cargo run -p wallet-service
cargo run -p order-service
cargo run -p market-data-service
cargo run -p gateway
```

### Smoke Test

```bash
# Register
curl -s -X POST http://localhost:8080/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"alice@example.com","password":"strongpassword123"}' | jq

# Login
TOKEN=$(curl -s -X POST http://localhost:8080/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"alice@example.com","password":"strongpassword123"}' | jq -r '.access_token')

# Check profile
curl -s http://localhost:8080/auth/me -H "Authorization: Bearer $TOKEN" | jq
```

### Development Commands

| Command | What it does |
|---------|-------------|
| `make up` / `make down` | start / stop all infra containers |
| `make migrate` | apply migrations to both DBs |
| `make migrate-info` | show migration status |
| `make db-reset` | DROP + recreate + re-migrate both DBs |
| `cargo test --workspace --all-targets` | all tests (unit + integration) |
| `cargo clippy --workspace --all-targets -- -D warnings` | lint (CI strictness) |
| `cargo fmt --all` | format check |

## API Documentation

Each service serves interactive Swagger UI documentation:

- Auth: `http://localhost:8080/docs/`
- Wallet: `http://localhost:8081/docs/`
- Order: `http://localhost:8082/docs/`
- Market Data: `http://localhost:8083/docs/`

OpenAPI JSON specs are available at `/api-docs/openapi.json` on each service.

### Endpoints Overview

#### Auth (`/auth/*`)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/auth/register` | No | Create account |
| POST | `/auth/login` | No | Get JWT token |
| GET | `/auth/me` | JWT | Current user profile |
| POST | `/auth/api-keys` | JWT | Create API key |

#### Wallet (`/wallets/*`)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/wallets` | JWT | List all wallets |
| GET | `/wallets/{currency}` | JWT | Get wallet by currency |
| POST | `/wallets/deposit` | JWT | Deposit funds |

#### Trading (`/orders/*`, `/pairs`)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/pairs` | No | Available trading pairs |
| POST | `/orders` | JWT | Place order (limit/market) |
| GET | `/orders` | JWT | List user's orders |
| GET | `/orders/{id}` | JWT | Get order by ID |
| DELETE | `/orders/{id}` | JWT | Cancel order |

#### Market Data (`/market/*`)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/market/ohlcv` | No | OHLCV candles |
| GET | `/market/orderbook/{pair}` | No | Latest orderbook snapshot |
| GET | `/market/trades/{pair}` | No | Recent trades |

#### WebSocket (Gateway)

Connect to `ws://localhost:8084/ws` with a channel subscription:

```json
{"channel": "trades", "pair": "BTC/USDT"}
{"channel": "orderbook", "pair": "BTC/USDT"}
{"channel": "orders", "token": "<JWT>"}
```

### Error Envelope

All error responses use a consistent format:

```json
{
  "code": "VALIDATION_ERROR",
  "message": "request validation failed",
  "details": { ... }
}
```

Codes: `VALIDATION_ERROR` (400), `UNAUTHORIZED` (401), `FORBIDDEN` (403), `NOT_FOUND` (404), `CONFLICT` (409), `INSUFFICIENT_BALANCE` (422), `PAIR_INACTIVE` (422), `INTERNAL_ERROR` (500).

## Configuration

`nexium-config` loads settings in this order (later wins):

1. `config/default.toml`
2. `config/{environment}.toml` — optional per-env overrides
3. Env vars `NEXIUM__SECTION__FIELD` (e.g. `NEXIUM__SERVER__PORT=9090`)
4. Standard vars `DATABASE_URL`, `TIMESCALE_URL`, `REDIS_URL`, `JWT_SECRET`
5. `service_name` passed to `AppConfig::load()`

`NEXIUM_ENV` is one of `local` (default), `development`, `production`. Production refuses to start with a weak `auth.jwt_secret`.

## Docker

### Build Production Image

```bash
docker build -t nexium:latest .
```

The multi-stage Dockerfile produces a minimal `debian:bookworm-slim` image containing all five service binaries. Run any service by overriding the entrypoint:

```bash
docker run -p 8080:8080 --env-file .env nexium:latest auth-service
docker run -p 8082:8080 --env-file .env nexium:latest order-service
```

### Full Stack (Development)

```bash
docker compose up -d   # postgres, timescaledb, redis, nats, jaeger
make migrate
```

## Observability

- **Traces:** OpenTelemetry OTLP → Jaeger at `http://localhost:16686`
- **Metrics:** Prometheus endpoint at `/metrics` on each service
- **Logs:** Structured JSON in development/production, pretty in local

## Project Layout

```
Cargo.toml              workspace root + shared deps
CLAUDE.md               AI assistant instructions
Makefile                docker compose + sqlx targets
Dockerfile              multi-stage production build
docker-compose.yml      postgres, timescaledb, redis, nats, jaeger
config/default.toml     baseline AppConfig
.env.example            template for local secrets

crates/
  core/                 domain types, errors, JWT, middleware, metrics, rate limiting
  config/               AppConfig loader (TOML + env + .env)
  telemetry/            tracing + OpenTelemetry init
  db/                   PgPool builders
  matching-engine/      pure orderbook + matching (no I/O)

services/
  auth/                 register, login, /me, API keys
  wallet/               balances, deposit, lock/unlock
  order/                place + cancel orders, matching, settlement, fees
  market-data/          OHLCV, orderbook snapshots, trade history
  gateway/              WebSocket real-time feeds

migrations/
  postgres/             auth + trading + wallet + fee schemas
  timescale/            market schema (hypertables)

docs/
  nexium.postman.json   Postman collection for all endpoints
```

## Sprint Progress

| # | Sprint | Goal | Status |
|---|--------|------|--------|
| 1 | Setup | Workspace, infra, migrations, CI | Done |
| 2 | Auth | Register / login / me / API keys | Done |
| 3 | Wallet | Wallets, deposit, lock/unlock | Done |
| 4 | Order | Place + cancel orders | Done |
| 5 | Matching engine | Orderbook + match logic | Done |
| 6 | Market data | OHLCV, snapshots, trade history | Done |
| 7 | WebSocket | Real-time push via NATS | Done |
| 8 | Hardening | Rate limit, OTel, Prometheus, fees | Done |
| 9 | Polish | OpenAPI docs, README, Docker, Postman | In progress |

## License

Private / portfolio project. Not licensed for production use.
