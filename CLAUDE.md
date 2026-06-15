# CLAUDE.md

Project-specific guidance for Claude Code (or any AI assistant) working in the Nexium repo. Keep this file concise — it is loaded into context on every turn.

## What this project is

**Nexium** — a cryptocurrency exchange backend in Rust. Solo learning / portfolio project: order matching, wallet management, real-time market data. Microservice-lite architecture (separate binaries communicating via REST / gRPC / messaging).

- **Repo:** `/Users/romando/Development/PersonalProject/Nexium` (GitHub: [MandoPurba/nexium](https://github.com/MandoPurba/nexium))
- **Stack:** Rust 1.85+, edition 2024, Actix-Web, sqlx 0.8, PostgreSQL 16, TimescaleDB, Redis 7. NATS + Kafka land in Sprint 5+.
- **Velocity:** ~1–2 hours/day, 8 sprints × 2 weeks, target finish Oct 2026.

## Documentation lives in Obsidian, not in the repo

Source of truth for **architecture, schema, API contracts, sprint plans, and design decisions** is the Obsidian vault at:

```
/Users/romando/Documents/Obsidian/Notes/01 - Projects/Nexium/
```

Always read the relevant Obsidian doc before implementing — the contract or diagram there is canonical, not memory.

| File | Purpose |
|---|---|
| `Nexium.md` | Project overview, sprint progress table, repo links |
| `Docs/System Architecture.md` | Service map, data-flow sequence diagrams |
| `Docs/Database Schema.md` | Schemas (`auth`, `trading`, `wallet`, `fee`, `market`), tables, ENUMs, indexes |
| `Docs/API Spec.md` | Endpoint request/response shapes, error envelope, codes, rate limits |
| `Docs/Tech Stack.md` | Pinned dependency versions, infra components |
| `Docs/Matching Engine Design.md` | Orderbook structure, matching algorithm |
| `Docs/Sprint Plan.md` | Task breakdown per sprint, DoD |
| `Docs/ADR Log.md` | Architecture decision records |
| `Docs/Dev Notes.md` | Free-form blockers / debugging notes |
| `Docs/Retrospectives.md` | End-of-sprint retros |
| `Docs/Sprints/Sprint N — *.md` | Per-sprint backlog & status |
| `Meetings/` | Meeting notes |

The repo itself only carries **executable artifacts** (code, migrations, configs, CI). Prose documentation stays in Obsidian.

## Repo layout

```
Cargo.toml             # workspace root + shared deps
CLAUDE.md              # this file
Makefile               # docker compose + sqlx targets
docker-compose.yml     # postgres, timescaledb, redis
config/default.toml    # baseline AppConfig
.env.example           # template for local secrets

crates/                # shared libraries
  core/                # domain types + errors (mostly empty so far)
  config/              # AppConfig loader (TOML + env + .env)
  telemetry/           # tracing-subscriber init
  db/                  # PgPool builders
  matching-engine/     # pure orderbook + matching (no I/O)

services/              # binary services — each its own crate
  auth/                # register, login, /me, JWT, API keys
  wallet/              # balances, deposit, lock/unlock
  order/               # place + cancel orders
  market-data/         # OHLCV, orderbook snapshots
  gateway/             # external HTTP + WS

migrations/
  postgres/            # auth + trading + wallet + fee schemas
  timescale/           # market schema (hypertables)

.github/workflows/ci.yml  # fmt + clippy + test
```

## Configuration

Layered loader in `nexium-config`. Precedence (later wins):

1. `config/default.toml`
2. `config/{environment}.toml` (e.g. `local`, `production`) — optional
3. Env vars `NEXIUM__SECTION__FIELD` (e.g. `NEXIUM__SERVER__PORT=9090`)
4. Standard vars `DATABASE_URL`, `TIMESCALE_URL`, `REDIS_URL`, `JWT_SECRET` — mapped to the corresponding fields so the same `.env` works for docker-compose, sqlx-cli, Makefile, and the app
5. `service_name` arg to `AppConfig::load()`

`Environment::{Local, Development, Production}` is parsed from `NEXIUM_ENV` (default `Local`). Production refuses to start with a weak `auth.jwt_secret`. Secrets are wrapped in `nexium_config::Secret<T>` — the `Debug` impl prints `Secret(***)`.

## Common commands

| Command | What it does |
|---|---|
| `make up` / `make down` | start / stop postgres + timescaledb + redis |
| `make migrate` | apply migrations to both DBs |
| `make migrate-info` | show migration status |
| `make db-reset` | DROP + recreate + re-migrate both DBs |
| `cargo run -p <service>` | run a service (`auth-service`, `wallet-service`, `order-service`, `market-data-service`, `gateway`) |
| `cargo test --workspace --all-targets` | all tests |
| `cargo clippy --workspace --all-targets -- -D warnings` | what CI runs |
| `cargo fmt --all` | what CI checks |

## API conventions

Every error response uses this envelope (see `Docs/API Spec.md`):

```json
{ "code": "VALIDATION_ERROR", "message": "...", "details": { ... } }
```

Codes: `VALIDATION_ERROR` (400) · `UNAUTHORIZED` (401) · `FORBIDDEN` (403) · `NOT_FOUND` (404) · `CONFLICT` (409) · `INSUFFICIENT_BALANCE` (422) · `PAIR_INACTIVE` (422) · `INTERNAL_ERROR` (500).

Money fields (prices, quantities, balances) are **strings of `NUMERIC(36, 18)`** end-to-end — never `f64`. Use `rust_decimal::Decimal` in Rust.

## Code conventions

**Bootstrap pattern** (every service `main.rs`):

```rust
let cfg = AppConfig::load(env!("CARGO_PKG_NAME"))?;
nexium_telemetry::init(&cfg.telemetry, cfg.environment)?;
let pool = nexium_db::pg_pool(&cfg.database).await?;
HttpServer::new(move || { ... }).bind((host, port))?.run().await?;
```

- **Postgres ENUMs** (`auth.user_status`, `trading.order_status`, etc.) — cast `::text` in SQL (`SELECT status::text AS status ...`) rather than implementing a custom `sqlx::Type`. Keeps sqlx setup boring.
- **Argon2** — use `Argon2::default()` (Argon2id, OWASP params). Always wrap `password::hash()` in `web::block(...)` — argon2 takes ~50ms and would stall the executor thread.
- **Email uniqueness** — normalize to lowercase + trim before insert / lookup.
- **Unique violation** — Postgres SQLSTATE `23505` → map to `409 Conflict`, not 500.
- **Tracing** — `tracing::info!()` etc; spans via `#[tracing::instrument(name = "...", skip_all, fields(...))]`. Output is pretty in `Local`, JSON in `Development` / `Production`. `RUST_LOG` overrides `telemetry.log_level`.
- **No `actix-web` `Logger` middleware** — request spans will come from `tracing-actix-web` in Sprint 8.
- **sqlx** — runtime queries (`query_as::<_, T>("...")`), not the compile-time `query!` macros. Keeps CI DB-free until integration tests land.

## Sprint plan (high-level)

| # | Sprint | Goal | Status |
|---|---|---|---|
| 1 | Setup | Workspace, infra, migrations, CI | ✅ Done |
| 2 | Auth | register / login / me / API keys | 🟡 In progress |
| 3 | Wallet | wallets, deposit, lock/unlock | |
| 4 | Order | place + cancel orders | |
| 5 | Matching engine | orderbook + match logic | |
| 6 | Market data | OHLCV, snapshots | |
| 7 | WebSocket | realtime push | |
| 8 | Hardening | rate limit, OpenTelemetry, metrics | |
| 9 | Polish | OpenAPI docs, deploy | |

Full backlog: `Docs/Sprint Plan.md`.

## Git

- Commit messages and PR bodies **must not** include `Co-Authored-By: Claude` lines or "Generated with Claude Code" attributions.
- Branches: `main` (stable) · `dev` (active) · `feature/*` (per task).
- Initial commit `fdc0637` predates this rule and still contains the attribution — leave it; the rule applies forward only.
