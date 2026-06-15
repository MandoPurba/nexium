-- Nexium initial schema for the TimescaleDB market-data database.
-- Schema: market.

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS timescaledb;

CREATE SCHEMA IF NOT EXISTS market;

-- ---------------------------------------------------------------------------
-- market.ohlcv  — candle bars per pair / interval
-- ---------------------------------------------------------------------------

CREATE TABLE market.ohlcv (
    pair     TEXT NOT NULL,
    interval TEXT NOT NULL,
    open     NUMERIC(36, 18) NOT NULL,
    high     NUMERIC(36, 18) NOT NULL,
    low      NUMERIC(36, 18) NOT NULL,
    close    NUMERIC(36, 18) NOT NULL,
    volume   NUMERIC(36, 18) NOT NULL,
    bucket   TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (pair, interval, bucket)
);

SELECT create_hypertable('market.ohlcv', 'bucket', if_not_exists => TRUE);

CREATE INDEX ohlcv_pair_interval_bucket_idx
    ON market.ohlcv (pair, interval, bucket DESC);

-- ---------------------------------------------------------------------------
-- market.order_book_snapshots  — periodic L2 snapshots per pair
-- Hypertable partition column must be in any UNIQUE/PK constraint, so the
-- PK is composite (id, captured_at) instead of just id.
-- ---------------------------------------------------------------------------

CREATE TABLE market.order_book_snapshots (
    id          UUID NOT NULL DEFAULT gen_random_uuid(),
    pair        TEXT NOT NULL,
    bids        JSONB NOT NULL,
    asks        JSONB NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (id, captured_at)
);

SELECT create_hypertable('market.order_book_snapshots', 'captured_at', if_not_exists => TRUE);

CREATE INDEX order_book_snapshots_pair_captured_idx
    ON market.order_book_snapshots (pair, captured_at DESC);
