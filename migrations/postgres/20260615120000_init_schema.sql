-- Nexium initial schema for the primary PostgreSQL database.
-- Schemas: auth, trading, wallet, fee.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE SCHEMA IF NOT EXISTS auth;
CREATE SCHEMA IF NOT EXISTS trading;
CREATE SCHEMA IF NOT EXISTS wallet;
CREATE SCHEMA IF NOT EXISTS fee;

-- ---------------------------------------------------------------------------
-- auth
-- ---------------------------------------------------------------------------

CREATE TYPE auth.user_status AS ENUM ('pending', 'active', 'suspended', 'banned');
CREATE TYPE auth.kyc_level   AS ENUM ('none', 'basic', 'advanced');
CREATE TYPE auth.kyc_status  AS ENUM ('pending', 'reviewing', 'approved', 'rejected');

CREATE TABLE auth.users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    status        auth.user_status NOT NULL DEFAULT 'pending',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE auth.kyc (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    level       auth.kyc_level  NOT NULL DEFAULT 'none',
    status      auth.kyc_status NOT NULL DEFAULT 'pending',
    documents   JSONB NOT NULL DEFAULT '{}'::jsonb,
    verified_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE auth.api_keys (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    key_hash     TEXT UNIQUE NOT NULL,
    permissions  TEXT[] NOT NULL DEFAULT '{}',
    expires_at   TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX api_keys_key_hash_idx ON auth.api_keys (key_hash);

-- ---------------------------------------------------------------------------
-- trading
-- ---------------------------------------------------------------------------

CREATE TYPE trading.order_side   AS ENUM ('buy', 'sell');
CREATE TYPE trading.order_type   AS ENUM ('limit', 'market', 'stop_limit', 'stop_market');
CREATE TYPE trading.order_status AS ENUM ('open', 'partially_filled', 'filled', 'cancelled', 'rejected');

CREATE TABLE trading.pairs (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol         TEXT UNIQUE NOT NULL,
    base_currency  TEXT NOT NULL,
    quote_currency TEXT NOT NULL,
    min_qty        NUMERIC(36, 18) NOT NULL,
    tick_size      NUMERIC(36, 18) NOT NULL,
    is_active      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE trading.orders (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID NOT NULL REFERENCES auth.users(id),
    pair       TEXT NOT NULL REFERENCES trading.pairs(symbol),
    side       trading.order_side   NOT NULL,
    type       trading.order_type   NOT NULL,
    status     trading.order_status NOT NULL DEFAULT 'open',
    price      NUMERIC(36, 18),
    quantity   NUMERIC(36, 18) NOT NULL CHECK (quantity > 0),
    filled_qty NUMERIC(36, 18) NOT NULL DEFAULT 0
        CHECK (filled_qty >= 0 AND filled_qty <= quantity),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX orders_user_status_created_idx ON trading.orders (user_id, status, created_at DESC);
CREATE INDEX orders_pair_status_created_idx ON trading.orders (pair, status, created_at DESC);

CREATE TABLE trading.trades (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    maker_order_id UUID NOT NULL REFERENCES trading.orders(id),
    taker_order_id UUID NOT NULL REFERENCES trading.orders(id),
    pair           TEXT NOT NULL,
    price          NUMERIC(36, 18) NOT NULL CHECK (price > 0),
    quantity       NUMERIC(36, 18) NOT NULL CHECK (quantity > 0),
    executed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX trades_pair_executed_idx ON trading.trades (pair, executed_at DESC);
CREATE INDEX trades_maker_order_idx   ON trading.trades (maker_order_id);
CREATE INDEX trades_taker_order_idx   ON trading.trades (taker_order_id);

-- ---------------------------------------------------------------------------
-- wallet
-- ---------------------------------------------------------------------------

CREATE TYPE wallet.txn_type   AS ENUM ('deposit', 'withdrawal', 'trade_debit', 'trade_credit', 'fee');
CREATE TYPE wallet.txn_status AS ENUM ('pending', 'confirmed', 'failed');

CREATE TABLE wallet.wallets (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id        UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    currency       TEXT NOT NULL,
    balance        NUMERIC(36, 18) NOT NULL DEFAULT 0 CHECK (balance >= 0),
    locked_balance NUMERIC(36, 18) NOT NULL DEFAULT 0 CHECK (locked_balance >= 0),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, currency)
);

CREATE TABLE wallet.wallet_txns (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_id  UUID NOT NULL REFERENCES wallet.wallets(id),
    type       wallet.txn_type   NOT NULL,
    amount     NUMERIC(36, 18) NOT NULL,
    status     wallet.txn_status NOT NULL DEFAULT 'pending',
    tx_hash    TEXT,
    ref_id     UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX wallet_txns_wallet_created_idx ON wallet.wallet_txns (wallet_id, created_at DESC);
CREATE INDEX wallet_txns_ref_idx            ON wallet.wallet_txns (ref_id);

-- ---------------------------------------------------------------------------
-- fee
-- ---------------------------------------------------------------------------

CREATE TABLE fee.fee_tiers (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    level          TEXT UNIQUE NOT NULL,
    maker_rate     NUMERIC(10, 6)  NOT NULL CHECK (maker_rate >= 0),
    taker_rate     NUMERIC(10, 6)  NOT NULL CHECK (taker_rate >= 0),
    min_volume_30d NUMERIC(36, 18) NOT NULL CHECK (min_volume_30d >= 0)
);

CREATE TABLE fee.fees (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID NOT NULL REFERENCES auth.users(id),
    trade_id   UUID NOT NULL REFERENCES trading.trades(id),
    currency   TEXT NOT NULL,
    amount     NUMERIC(36, 18) NOT NULL CHECK (amount >= 0),
    rate       NUMERIC(10, 6)  NOT NULL CHECK (rate >= 0),
    type       TEXT NOT NULL CHECK (type IN ('maker', 'taker')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX fees_trade_idx ON fee.fees (trade_id);
CREATE INDEX fees_user_idx  ON fee.fees (user_id, created_at DESC);
