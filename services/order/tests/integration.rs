//! Integration tests for the order service.
//!
//! Each `#[sqlx::test]` boots a throwaway database, runs migrations (including
//! the pair seed migration), and hands back a [`PgPool`]. Tests seed users,
//! wallets, and KYC rows as needed — they share no state with each other.

use actix_web::{
    App,
    test::{TestRequest, call_service, init_service, read_body_json},
    web,
};
use nexium_core::jwt::JwtIssuer;
use order_service::{EngineSender, configure, spawn_engine};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;
use wallet_service::repository as wallet_repo;

const TEST_SECRET: &str = "integration-test-secret-not-used-in-prod";
const TEST_EXPIRY_SECS: u64 = 3600;

fn issuer() -> JwtIssuer {
    JwtIssuer::new(TEST_SECRET, TEST_EXPIRY_SECS)
}

fn engine_handle(pool: &PgPool) -> EngineSender {
    spawn_engine(pool.clone(), None, None)
}

/// Convenience macro — builds the full `init_service` test app with engine
/// wired in. Always clones the pool so callers can still inspect the DB after.
macro_rules! build_app {
    ($pool:expr) => {{
        let pool = $pool.clone();
        let engine_tx = engine_handle(&pool);
        init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .app_data(web::Data::new(issuer()))
                .app_data(web::Data::new(engine_tx))
                .configure(configure),
        )
        .await
    }};
}

// ---------------------------------------------------------------------------
// Seeding helpers
// ---------------------------------------------------------------------------

/// Insert an `active` user, provision default wallets, and return a bearer token.
async fn seed_active_user(pool: &PgPool) -> (Uuid, String) {
    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO auth.users (id, email, password_hash, status) VALUES ($1, $2, 'hash', 'active')",
    )
    .bind(user_id)
    .bind(format!("{user_id}@example.com"))
    .execute(pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO auth.kyc (user_id, level, status) VALUES ($1, 'basic', 'approved')")
        .bind(user_id)
        .execute(pool)
        .await
        .unwrap();

    wallet_repo::create_default_wallets(pool, user_id)
        .await
        .unwrap();

    let (token, _) = issuer().issue(user_id).unwrap();
    (user_id, token)
}

/// Insert a `pending` user with no KYC (cannot trade).
async fn seed_pending_user(pool: &PgPool) -> (Uuid, String) {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO auth.users (id, email, password_hash) VALUES ($1, $2, 'hash')")
        .bind(user_id)
        .bind(format!("{user_id}@example.com"))
        .execute(pool)
        .await
        .unwrap();

    let (token, _) = issuer().issue(user_id).unwrap();
    (user_id, token)
}

/// Poll a DB predicate until it returns true or the timeout expires. Used to
/// give the async settlement task time to apply its effects.
async fn await_condition<F, Fut>(timeout: Duration, mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if check().await {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---------------------------------------------------------------------------
// GET /pairs
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn list_pairs_returns_seeded_pairs(pool: PgPool) {
    let app = build_app!(pool);

    let resp = call_service(&app, TestRequest::get().uri("/pairs").to_request()).await;
    assert_eq!(resp.status(), 200);

    let body: Value = read_body_json(resp).await;
    let symbols: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["symbol"].as_str().unwrap())
        .collect();
    assert!(symbols.contains(&"BTC/USDT"));
    assert!(symbols.contains(&"ETH/USDT"));
}

// ---------------------------------------------------------------------------
// POST /orders — happy paths
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn place_limit_buy_locks_quote_balance(pool: PgPool) {
    let (user_id, token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, user_id, "USDT", Decimal::from(1000))
        .await
        .unwrap();

    let app = build_app!(pool);

    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "pair": "BTC/USDT",
                "side": "buy",
                "type": "limit",
                "price": "65000",
                "quantity": "0.01"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["status"], "open");

    let wallet = wallet_repo::find_by_currency(&pool, user_id, "USDT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(wallet.balance, Decimal::from(350));
    assert_eq!(wallet.locked_balance, Decimal::from(650));
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn place_limit_sell_locks_base_balance(pool: PgPool) {
    let (user_id, token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, user_id, "BTC", Decimal::from(1))
        .await
        .unwrap();

    let app = build_app!(pool);

    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({
                "pair": "BTC/USDT",
                "side": "sell",
                "type": "limit",
                "price": "65000",
                "quantity": "0.5"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);

    let wallet = wallet_repo::find_by_currency(&pool, user_id, "BTC")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(wallet.balance, Decimal::from_str_exact("0.5").unwrap());
    assert_eq!(
        wallet.locked_balance,
        Decimal::from_str_exact("0.5").unwrap()
    );
}

// ---------------------------------------------------------------------------
// POST /orders — error paths
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn place_order_without_auth_returns_401(pool: PgPool) {
    let app = build_app!(pool);

    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 401);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn place_order_pending_user_returns_403(pool: PgPool) {
    let (_, token) = seed_pending_user(&pool).await;

    let app = build_app!(pool);

    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 403);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["code"], "FORBIDDEN");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn place_order_insufficient_balance_returns_422(pool: PgPool) {
    let (_, token) = seed_active_user(&pool).await;
    // No deposit → USDT balance is 0.

    let app = build_app!(pool);

    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 422);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["code"], "INSUFFICIENT_BALANCE");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn place_order_unknown_pair_returns_404(pool: PgPool) {
    let (_, token) = seed_active_user(&pool).await;

    let app = build_app!(pool);

    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"pair": "DOGE/USDT", "side": "buy", "type": "limit",
                             "price": "1", "quantity": "100"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn place_limit_order_without_price_returns_400(pool: PgPool) {
    let (_, token) = seed_active_user(&pool).await;

    let app = build_app!(pool);

    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
}

// ---------------------------------------------------------------------------
// GET /orders + GET /orders/:id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn list_orders_returns_users_orders(pool: PgPool) {
    let (user_id, token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, user_id, "USDT", Decimal::from(2000))
        .await
        .unwrap();

    let app = build_app!(pool);

    for qty in ["0.01", "0.02"] {
        call_service(
            &app,
            TestRequest::post()
                .uri("/orders")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                                 "price": "65000", "quantity": qty}))
                .to_request(),
        )
        .await;
    }

    let resp = call_service(
        &app,
        TestRequest::get()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn get_order_by_id_returns_order(pool: PgPool) {
    let (user_id, token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, user_id, "USDT", Decimal::from(1000))
        .await
        .unwrap();

    let app = build_app!(pool);

    let place_resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    let place_body: Value = read_body_json(place_resp).await;
    let order_id = place_body["id"].as_str().unwrap();

    let resp = call_service(
        &app,
        TestRequest::get()
            .uri(&format!("/orders/{order_id}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["id"], order_id);
    assert_eq!(body["status"], "open");
}

// ---------------------------------------------------------------------------
// DELETE /orders/:id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn cancel_order_unlocks_balance(pool: PgPool) {
    let (user_id, token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, user_id, "USDT", Decimal::from(1000))
        .await
        .unwrap();

    let app = build_app!(pool);

    let place_resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    let place_body: Value = read_body_json(place_resp).await;
    let order_id = place_body["id"].as_str().unwrap();

    let cancel_resp = call_service(
        &app,
        TestRequest::delete()
            .uri(&format!("/orders/{order_id}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(cancel_resp.status(), 200);

    let cancel_body: Value = read_body_json(cancel_resp).await;
    assert_eq!(cancel_body["status"], "cancelled");

    let wallet = wallet_repo::find_by_currency(&pool, user_id, "USDT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(wallet.balance, Decimal::from(1000));
    assert_eq!(wallet.locked_balance, Decimal::ZERO);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn cancel_already_cancelled_returns_404(pool: PgPool) {
    let (user_id, token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, user_id, "USDT", Decimal::from(1000))
        .await
        .unwrap();

    let app = build_app!(pool);

    let place_resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    let place_body: Value = read_body_json(place_resp).await;
    let order_id = place_body["id"].as_str().unwrap();

    call_service(
        &app,
        TestRequest::delete()
            .uri(&format!("/orders/{order_id}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;

    let resp = call_service(
        &app,
        TestRequest::delete()
            .uri(&format!("/orders/{order_id}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn cancel_other_users_order_returns_404(pool: PgPool) {
    let (user_a, token_a) = seed_active_user(&pool).await;
    let (_, token_b) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, user_a, "USDT", Decimal::from(1000))
        .await
        .unwrap();

    let app = build_app!(pool);

    let place_resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {token_a}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    let place_body: Value = read_body_json(place_resp).await;
    let order_id = place_body["id"].as_str().unwrap();

    let resp = call_service(
        &app,
        TestRequest::delete()
            .uri(&format!("/orders/{order_id}"))
            .insert_header(("Authorization", format!("Bearer {token_b}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// Matching + settlement — end-to-end
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn matching_two_orders_settles_trade_and_wallets(pool: PgPool) {
    // Seller: Alice — holds 1 BTC, places a sell limit at 65000.
    let (alice, alice_token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, alice, "BTC", Decimal::from(1))
        .await
        .unwrap();

    // Buyer: Bob — holds 1000 USDT, places a crossing buy at 65000.
    let (bob, bob_token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, bob, "USDT", Decimal::from(1000))
        .await
        .unwrap();

    let app = build_app!(pool);

    // Alice rests an ask: 0.01 BTC @ 65000 USDT.
    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {alice_token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "sell", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let alice_order_id: String = read_body_json::<Value, _>(resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Bob takes — buy 0.01 BTC @ 65000 USDT.
    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {bob_token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let bob_order_id: String = read_body_json::<Value, _>(resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Settlement is async — poll until a trade row appears (or fail).
    let pool_probe = pool.clone();
    let settled = await_condition(Duration::from_secs(3), || {
        let pool = pool_probe.clone();
        async move {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trading.trades")
                .fetch_one(&pool)
                .await
                .unwrap();
            count > 0
        }
    })
    .await;
    assert!(settled, "settlement task should have inserted a trade row");

    // Trade exists with the right shape.
    let trade: (Decimal, Decimal, String) = sqlx::query_as(
        "SELECT price, quantity, pair FROM trading.trades ORDER BY executed_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(trade.0, Decimal::from(65000));
    assert_eq!(trade.1, Decimal::from_str_exact("0.01").unwrap());
    assert_eq!(trade.2, "BTC/USDT");

    // Both orders are filled.
    let alice_status: String =
        sqlx::query_scalar("SELECT status::text FROM trading.orders WHERE id = $1::uuid")
            .bind(&alice_order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let bob_status: String =
        sqlx::query_scalar("SELECT status::text FROM trading.orders WHERE id = $1::uuid")
            .bind(&bob_order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(alice_status, "filled");
    assert_eq!(bob_status, "filled");

    // trade_value = 650, maker_fee = 0.001 × 650 = 0.65, taker_fee = 0.002 × 650 = 1.30
    // Wallets: Alice gives up 0.01 BTC, gets 650 - 0.65 = 649.35 USDT.
    let alice_btc = wallet_repo::find_by_currency(&pool, alice, "BTC")
        .await
        .unwrap()
        .unwrap();
    let alice_usdt = wallet_repo::find_by_currency(&pool, alice, "USDT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(alice_btc.balance, Decimal::from_str_exact("0.99").unwrap());
    assert_eq!(alice_btc.locked_balance, Decimal::ZERO);
    assert_eq!(
        alice_usdt.balance,
        Decimal::from_str_exact("649.35").unwrap()
    );

    // Bob: pays 650 USDT + 1.30 fee, gets 0.01 BTC. No leftover lock.
    let bob_btc = wallet_repo::find_by_currency(&pool, bob, "BTC")
        .await
        .unwrap()
        .unwrap();
    let bob_usdt = wallet_repo::find_by_currency(&pool, bob, "USDT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bob_btc.balance, Decimal::from_str_exact("0.01").unwrap());
    assert_eq!(bob_usdt.balance, Decimal::from_str_exact("348.70").unwrap());
    assert_eq!(bob_usdt.locked_balance, Decimal::ZERO);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn buy_at_higher_price_refunds_difference_on_settle(pool: PgPool) {
    // Alice sells at 64900 (resting maker). Bob buys at 65000 (crosses higher).
    // Trade should execute at the maker price (64900), and Bob's quote
    // overlock (100 × 0.01 = 1 USDT) should be refunded to available.
    let (alice, alice_token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, alice, "BTC", Decimal::from(1))
        .await
        .unwrap();
    let (bob, bob_token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, bob, "USDT", Decimal::from(1000))
        .await
        .unwrap();

    let app = build_app!(pool);

    call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {alice_token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "sell", "type": "limit",
                             "price": "64900", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {bob_token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;

    let pool_probe = pool.clone();
    let settled = await_condition(Duration::from_secs(3), || {
        let pool = pool_probe.clone();
        async move {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trading.trades")
                .fetch_one(&pool)
                .await
                .unwrap();
            count > 0
        }
    })
    .await;
    assert!(settled);

    // Trade executed at the maker price.
    let trade_price: Decimal =
        sqlx::query_scalar("SELECT price FROM trading.trades ORDER BY executed_at DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(trade_price, Decimal::from(64900));

    // trade_value = 649, maker_fee = 0.001 × 649 = 0.649, taker_fee = 0.002 × 649 = 1.298
    // Bob: locked 650 (65000 × 0.01), actually paid 649 (64900 × 0.01),
    // 1 USDT refund, then taker_fee deducted → balance = 1000 - 650 + 1 - 1.298 = 349.702.
    let bob_usdt = wallet_repo::find_by_currency(&pool, bob, "USDT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        bob_usdt.balance,
        Decimal::from_str_exact("349.702").unwrap()
    );
    assert_eq!(bob_usdt.locked_balance, Decimal::ZERO);

    // Alice received 649 - 0.649 = 648.351 USDT.
    let alice_usdt = wallet_repo::find_by_currency(&pool, alice, "USDT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        alice_usdt.balance,
        Decimal::from_str_exact("648.351").unwrap()
    );
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn partial_fill_status_is_partially_filled(pool: PgPool) {
    // Alice sells 0.005 BTC. Bob buys 0.01 BTC. Bob's order partially fills
    // (0.005 done, 0.005 still resting); Alice's order fully fills.
    let (alice, alice_token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, alice, "BTC", Decimal::from(1))
        .await
        .unwrap();
    let (bob, bob_token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, bob, "USDT", Decimal::from(1000))
        .await
        .unwrap();

    let app = build_app!(pool);

    call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {alice_token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "sell", "type": "limit",
                             "price": "65000", "quantity": "0.005"}))
            .to_request(),
    )
    .await;
    let bob_resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {bob_token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    let bob_order_id: String = read_body_json::<Value, _>(bob_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let pool_probe = pool.clone();
    let settled = await_condition(Duration::from_secs(3), || {
        let pool = pool_probe.clone();
        let bob_order_id = bob_order_id.clone();
        async move {
            let status: Option<String> =
                sqlx::query_scalar("SELECT status::text FROM trading.orders WHERE id = $1::uuid")
                    .bind(&bob_order_id)
                    .fetch_optional(&pool)
                    .await
                    .unwrap();
            status.as_deref() == Some("partially_filled")
        }
    })
    .await;
    assert!(settled, "bob's order should end up partially_filled");

    let bob_filled: Decimal =
        sqlx::query_scalar("SELECT filled_qty FROM trading.orders WHERE id = $1::uuid")
            .bind(&bob_order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(bob_filled, Decimal::from_str_exact("0.005").unwrap());

    // Suppress unused-variable warnings; these are seeded for side-effect.
    let _ = (alice, bob);
}

// ---------------------------------------------------------------------------
// E2E: register → deposit → order → match → balance + fee check
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn e2e_register_deposit_order_match_with_fees(pool: PgPool) {
    // Seed fee tiers (normally done by migration, but sqlx::test only runs
    // the migration files — our seed migration covers this).

    // Seller: Alice — deposit 1 BTC, sell 0.01 @ 65000
    let (alice, alice_token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, alice, "BTC", Decimal::from(1))
        .await
        .unwrap();

    // Buyer: Bob — deposit 1000 USDT, buy 0.01 @ 65000
    let (bob, bob_token) = seed_active_user(&pool).await;
    wallet_repo::deposit(&pool, bob, "USDT", Decimal::from(1000))
        .await
        .unwrap();

    let app = build_app!(pool);

    // Alice places sell
    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {alice_token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "sell", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);

    // Bob places crossing buy
    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/orders")
            .insert_header(("Authorization", format!("Bearer {bob_token}")))
            .set_json(json!({"pair": "BTC/USDT", "side": "buy", "type": "limit",
                             "price": "65000", "quantity": "0.01"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);

    // Wait for settlement + fees
    let pool_probe = pool.clone();
    let settled = await_condition(Duration::from_secs(3), || {
        let pool = pool_probe.clone();
        async move {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fee.fees")
                .fetch_one(&pool)
                .await
                .unwrap();
            count >= 2
        }
    })
    .await;
    assert!(settled, "fee records should be inserted after settlement");

    // trade_value = 65000 * 0.01 = 650
    // maker_fee = 0.001 * 650 = 0.65
    // taker_fee = 0.002 * 650 = 1.30
    let trade_value = Decimal::from(650);
    let maker_fee = Decimal::from_str_exact("0.65").unwrap();
    let taker_fee = Decimal::from_str_exact("1.3").unwrap();

    // Fee records exist
    let fee_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fee.fees")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fee_count, 2);

    // Alice (maker/seller): receives trade_value in USDT, minus maker_fee
    let alice_usdt = wallet_repo::find_by_currency(&pool, alice, "USDT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(alice_usdt.balance, trade_value - maker_fee);

    // Bob (taker/buyer): had 1000, locked 650, paid 650, fee deducted from remaining
    // balance = 1000 - 650 (lock) + 0 (refund, same price) - taker_fee
    let bob_usdt = wallet_repo::find_by_currency(&pool, bob, "USDT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bob_usdt.balance, Decimal::from(350) - taker_fee);
    assert_eq!(bob_usdt.locked_balance, Decimal::ZERO);

    // Bob received 0.01 BTC
    let bob_btc = wallet_repo::find_by_currency(&pool, bob, "BTC")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bob_btc.balance, Decimal::from_str_exact("0.01").unwrap());

    // Alice BTC: 1 - 0.01 (sold) = 0.99, no lock
    let alice_btc = wallet_repo::find_by_currency(&pool, alice, "BTC")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(alice_btc.balance, Decimal::from_str_exact("0.99").unwrap());
    assert_eq!(alice_btc.locked_balance, Decimal::ZERO);
}
