//! Integration tests for the wallet service.
//!
//! Each `#[sqlx::test]` boots a throwaway database, runs the postgres
//! migrations against it, and hands back a [`PgPool`] — tests are
//! independent and don't share state. Since the wallet service has no
//! register endpoint of its own, tests seed a bare `auth.users` row directly.

use actix_web::{
    App,
    test::{TestRequest, call_service, init_service, read_body_json},
    web,
};
use nexium_core::jwt::JwtIssuer;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wallet_service::{configure, repository};

const TEST_SECRET: &str = "integration-test-secret-not-used-in-prod";
const TEST_EXPIRY_SECS: u64 = 3600;

fn issuer() -> JwtIssuer {
    JwtIssuer::new(TEST_SECRET, TEST_EXPIRY_SECS)
}

/// Insert a bare user row, provision their default wallets, and return the
/// user id alongside a bearer token for it.
async fn seed_user(pool: &PgPool) -> (Uuid, String) {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO auth.users (id, email, password_hash) VALUES ($1, $2, 'hash')")
        .bind(user_id)
        .bind(format!("{user_id}@example.com"))
        .execute(pool)
        .await
        .unwrap();
    repository::create_default_wallets(pool, user_id)
        .await
        .unwrap();

    let (token, _) = issuer().issue(user_id).unwrap();
    (user_id, token)
}

fn decimal(value: &Value) -> Decimal {
    value.as_str().unwrap().parse().unwrap()
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn list_wallets_returns_default_currencies(pool: PgPool) {
    let (_, token) = seed_user(&pool).await;
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let resp = call_service(
        &app,
        TestRequest::get()
            .uri("/wallets")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);

    let body: Value = read_body_json(resp).await;
    let wallets = body.as_array().unwrap();
    let currencies: Vec<&str> = wallets
        .iter()
        .map(|w| w["currency"].as_str().unwrap())
        .collect();
    assert_eq!(currencies, vec!["BTC", "ETH", "USDT"]);

    for wallet in wallets {
        assert_eq!(decimal(&wallet["balance"]), Decimal::ZERO);
        assert_eq!(decimal(&wallet["locked_balance"]), Decimal::ZERO);
        assert_eq!(decimal(&wallet["available"]), Decimal::ZERO);
        assert!(wallet["id"].is_string());
    }
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn get_wallet_by_currency_is_case_insensitive(pool: PgPool) {
    let (_, token) = seed_user(&pool).await;
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let resp = call_service(
        &app,
        TestRequest::get()
            .uri("/wallets/usdt")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["currency"], "USDT");
    assert_eq!(decimal(&body["balance"]), Decimal::ZERO);
    assert_eq!(decimal(&body["available"]), Decimal::ZERO);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn get_wallet_unknown_currency_returns_404(pool: PgPool) {
    let (_, token) = seed_user(&pool).await;
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let resp = call_service(
        &app,
        TestRequest::get()
            .uri("/wallets/DOGE")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["code"], "NOT_FOUND");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn deposit_increases_balance(pool: PgPool) {
    let (_, token) = seed_user(&pool).await;
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
            .uri("/wallets/deposit")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"currency": "usdt", "amount": "1000"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["currency"], "USDT");
    assert_eq!(body["status"], "confirmed");
    assert_eq!(decimal(&body["amount"]), Decimal::from(1000));
    assert!(body["txn_id"].is_string());

    let resp = call_service(
        &app,
        TestRequest::get()
            .uri("/wallets/USDT")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let body: Value = read_body_json(resp).await;
    assert_eq!(decimal(&body["balance"]), Decimal::from(1000));
    assert_eq!(decimal(&body["available"]), Decimal::from(1000));
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn deposit_non_positive_amount_returns_400(pool: PgPool) {
    let (_, token) = seed_user(&pool).await;
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
            .uri("/wallets/deposit")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"currency": "USDT", "amount": "0"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn deposit_unknown_currency_returns_404(pool: PgPool) {
    let (_, token) = seed_user(&pool).await;
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
            .uri("/wallets/deposit")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"currency": "DOGE", "amount": "10"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["code"], "NOT_FOUND");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn wallets_without_authorization_returns_401(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let resp = call_service(&app, TestRequest::get().uri("/wallets").to_request()).await;
    assert_eq!(resp.status(), 401);

    let body: Value = read_body_json(resp).await;
    assert_eq!(body["code"], "UNAUTHORIZED");
}
