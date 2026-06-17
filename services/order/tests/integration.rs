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
use order_service::configure;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wallet_service::repository as wallet_repo;

const TEST_SECRET: &str = "integration-test-secret-not-used-in-prod";
const TEST_EXPIRY_SECS: u64 = 3600;

fn issuer() -> JwtIssuer {
    JwtIssuer::new(TEST_SECRET, TEST_EXPIRY_SECS)
}

// ---------------------------------------------------------------------------
// Helpers
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

    // KYC basic approved — required for the trading role guard.
    sqlx::query(
        r#"
        INSERT INTO auth.kyc (user_id, level, status)
        VALUES ($1, 'basic', 'approved')
        "#,
    )
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

// ---------------------------------------------------------------------------
// GET /pairs
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn list_pairs_returns_seeded_pairs(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

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
    // Deposit 1000 USDT so the user can afford the order.
    wallet_repo::deposit(&pool, user_id, "USDT", Decimal::from(1000))
        .await
        .unwrap();

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    // Buy 0.01 BTC at 65000 USDT → lock 650 USDT.
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
    assert_eq!(body["pair"], "BTC/USDT");
    assert_eq!(body["side"], "buy");
    assert_eq!(body["type"], "limit");

    // Verify 650 USDT is locked.
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

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    // Sell 0.5 BTC at 65000 → lock 0.5 BTC.
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
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

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

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

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

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

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

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

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

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

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

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    // Place two orders.
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

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    // Place an order and capture its ID.
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

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    // Place a buy limit order: locks 650 USDT.
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

    // Cancel the order.
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

    // Balance should be fully restored.
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

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

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

    // First cancel succeeds.
    call_service(
        &app,
        TestRequest::delete()
            .uri(&format!("/orders/{order_id}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;

    // Second cancel fails.
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

    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

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

    // User B tries to cancel user A's order.
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
