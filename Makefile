SHELL := /bin/bash
.ONESHELL:
.DEFAULT_GOAL := help

ifneq (,$(wildcard .env))
include .env
export
endif

DATABASE_URL  ?= postgres://nexium:nexium@localhost:5432/nexium
TIMESCALE_URL ?= postgres://nexium:nexium@localhost:5433/nexium_market

.PHONY: help up down logs ps \
        migrate migrate-pg migrate-ts \
        migrate-revert migrate-revert-pg migrate-revert-ts \
        migrate-info migrate-info-pg migrate-info-ts \
        db-reset db-reset-pg db-reset-ts

help: ## Show this help.
	@awk 'BEGIN {FS = ":.*##"; printf "Available targets:\n"} \
	      /^[a-zA-Z0-9_-]+:.*?##/ { printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

# --- docker compose -------------------------------------------------------

up:        ## Start infrastructure (postgres, timescaledb, redis).
	docker compose up -d

down:      ## Stop infrastructure.
	docker compose down

logs:      ## Tail logs from all infra containers.
	docker compose logs -f

ps:        ## Show running infra containers.
	docker compose ps

# --- migrations -----------------------------------------------------------

migrate: migrate-pg migrate-ts ## Apply migrations to both DBs.

migrate-pg: ## Apply migrations to PostgreSQL.
	sqlx migrate run --source migrations/postgres --database-url $(DATABASE_URL)

migrate-ts: ## Apply migrations to TimescaleDB.
	sqlx migrate run --source migrations/timescale --database-url $(TIMESCALE_URL)

migrate-revert: migrate-revert-ts migrate-revert-pg ## Revert most recent migration on both DBs.

migrate-revert-pg: ## Revert most recent migration on PostgreSQL.
	sqlx migrate revert --source migrations/postgres --database-url $(DATABASE_URL)

migrate-revert-ts: ## Revert most recent migration on TimescaleDB.
	sqlx migrate revert --source migrations/timescale --database-url $(TIMESCALE_URL)

migrate-info: migrate-info-pg migrate-info-ts ## Show migration status for both DBs.

migrate-info-pg: ## Show migration status for PostgreSQL.
	sqlx migrate info --source migrations/postgres --database-url $(DATABASE_URL)

migrate-info-ts: ## Show migration status for TimescaleDB.
	sqlx migrate info --source migrations/timescale --database-url $(TIMESCALE_URL)

# --- reset (DROP + recreate + re-migrate) ---------------------------------

db-reset: db-reset-pg db-reset-ts ## DROP + recreate + re-migrate both DBs.

db-reset-pg: ## DROP + recreate + re-migrate PostgreSQL.
	sqlx database drop -y --database-url $(DATABASE_URL)
	sqlx database create     --database-url $(DATABASE_URL)
	$(MAKE) migrate-pg

db-reset-ts: ## DROP + recreate + re-migrate TimescaleDB.
	sqlx database drop -y --database-url $(TIMESCALE_URL)
	sqlx database create     --database-url $(TIMESCALE_URL)
	$(MAKE) migrate-ts
