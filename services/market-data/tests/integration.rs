use actix_web::{
    App,
    test::{TestRequest, call_service, init_service, read_body_json},
    web,
};
use chrono::Utc;
use market_data_service::{configure, routes::TimescalePool};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers — we run against the primary postgres for simplicity; the tests
// seed market.* tables manually (the TimescaleDB-only CREATE EXTENSION is
// skipped, but the raw DDL still works on plain postgres).
// ---------------------------------------------------------------------------

macro_rules! build_app {
    ($pool:expr) => {{
        let pool = $pool.clone();
        init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .app_data(web::Data::new(TimescalePool(pool)))
                .configure(configure),
        )
        .await
    }};
}

async fn ensure_market_schema(pool: &PgPool) {
    sqlx::query("CREATE SCHEMA IF NOT EXISTS market")
        .execute(pool)
        .await
        .unwrap();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS market.ohlcv (
            pair     TEXT NOT NULL,
            interval TEXT NOT NULL,
            open     NUMERIC(36,18) NOT NULL,
            high     NUMERIC(36,18) NOT NULL,
            low      NUMERIC(36,18) NOT NULL,
            close    NUMERIC(36,18) NOT NULL,
            volume   NUMERIC(36,18) NOT NULL,
            bucket   TIMESTAMPTZ NOT NULL,
            PRIMARY KEY (pair, interval, bucket)
        )
        "#,
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS market.order_book_snapshots (
            id          UUID NOT NULL DEFAULT gen_random_uuid(),
            pair        TEXT NOT NULL,
            bids        JSONB NOT NULL,
            asks        JSONB NOT NULL,
            captured_at TIMESTAMPTZ NOT NULL,
            PRIMARY KEY (id, captured_at)
        )
        "#,
    )
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// GET /market/ohlcv
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn ohlcv_returns_candles(pool: PgPool) {
    ensure_market_schema(&pool).await;

    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO market.ohlcv (pair, interval, open, high, low, close, volume, bucket)
        VALUES ('BTC/USDT', '1h', 65000, 65200, 64800, 65100, 10.5, $1)
        "#,
    )
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_app!(pool);
    let req = TestRequest::get()
        .uri("/market/ohlcv?pair=BTC/USDT&interval=1h")
        .to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = read_body_json(resp).await;
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["pair"], "BTC/USDT");
    assert_eq!(body[0]["interval"], "1h");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn ohlcv_rejects_invalid_interval(pool: PgPool) {
    ensure_market_schema(&pool).await;

    let app = build_app!(pool);
    let req = TestRequest::get()
        .uri("/market/ohlcv?pair=BTC/USDT&interval=2h")
        .to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn ohlcv_empty_when_no_data(pool: PgPool) {
    ensure_market_schema(&pool).await;

    let app = build_app!(pool);
    let req = TestRequest::get()
        .uri("/market/ohlcv?pair=BTC/USDT&interval=1h")
        .to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = read_body_json(resp).await;
    assert!(body.is_empty());
}

// ---------------------------------------------------------------------------
// GET /market/orderbook/:pair
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn orderbook_returns_latest_snapshot(pool: PgPool) {
    ensure_market_schema(&pool).await;

    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO market.order_book_snapshots (pair, bids, asks, captured_at)
        VALUES ('BTC/USDT', '[["65000","0.5"]]'::jsonb, '[["65100","0.3"]]'::jsonb, $1)
        "#,
    )
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_app!(pool);
    let req = TestRequest::get()
        .uri("/market/orderbook/BTC%2FUSDT")
        .to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["pair"], "BTC/USDT");
    assert!(body["bids"].is_array());
    assert!(body["asks"].is_array());
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn orderbook_404_when_no_snapshot(pool: PgPool) {
    ensure_market_schema(&pool).await;

    let app = build_app!(pool);
    let req = TestRequest::get()
        .uri("/market/orderbook/BTC%2FUSDT")
        .to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// GET /market/trades/:pair
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn trades_returns_recent_trades(pool: PgPool) {
    let user = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO auth.users (id, email, password_hash, status) VALUES ($1, $2, 'h', 'active')",
    )
    .bind(user)
    .bind(format!("{user}@test.com"))
    .execute(&pool)
    .await
    .unwrap();

    let order_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO trading.orders (id, user_id, pair, side, type, status, price, quantity, filled_qty)
        VALUES ($1, $2, 'BTC/USDT', 'buy', 'limit', 'filled', 65000, 1, 1)
        "#,
    )
    .bind(order_id)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();

    let order_id2 = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO trading.orders (id, user_id, pair, side, type, status, price, quantity, filled_qty)
        VALUES ($1, $2, 'BTC/USDT', 'sell', 'limit', 'filled', 65000, 1, 1)
        "#,
    )
    .bind(order_id2)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();

    let trade_id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO trading.trades (id, maker_order_id, taker_order_id, pair, price, quantity, executed_at)
        VALUES ($1, $2, $3, 'BTC/USDT', 65000, 1, $4)
        "#,
    )
    .bind(trade_id)
    .bind(order_id2)
    .bind(order_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_app!(pool);
    let req = TestRequest::get()
        .uri("/market/trades/BTC%2FUSDT?limit=10")
        .to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = read_body_json(resp).await;
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["pair"], "BTC/USDT");
    assert_eq!(body[0]["side"], "buy");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn trades_empty_when_no_trades(pool: PgPool) {
    let app = build_app!(pool);
    let req = TestRequest::get()
        .uri("/market/trades/BTC%2FUSDT")
        .to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = read_body_json(resp).await;
    assert!(body.is_empty());
}
