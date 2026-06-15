//! Integration tests for the auth service.
//!
//! Each `#[sqlx::test]` boots a throwaway database, runs the postgres
//! migrations against it, and hands back a [`PgPool`] — tests are
//! independent and don't share state.

use actix_web::{
    App,
    test::{TestRequest, call_service, init_service, read_body_json},
    web,
};
use auth_service::{configure, jwt::JwtIssuer};
use serde_json::{Value, json};
use sqlx::PgPool;

const TEST_SECRET: &str = "integration-test-secret-not-used-in-prod";
const TEST_EXPIRY_SECS: u64 = 3600;

fn issuer() -> JwtIssuer {
    JwtIssuer::new(TEST_SECRET, TEST_EXPIRY_SECS)
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn register_login_me_happy_path(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    // 1. Register
    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/auth/register")
            .set_json(json!({"email": "alice@example.com", "password": "strongpassword123"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201, "register expected 201");
    let body: Value = read_body_json(resp).await;
    assert_eq!(body["email"], "alice@example.com");
    assert_eq!(body["status"], "pending");
    assert!(body["id"].is_string());
    assert!(body["created_at"].is_string());

    // 2. Login
    let resp = call_service(
        &app,
        TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"email": "alice@example.com", "password": "strongpassword123"}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200, "login expected 200");
    let body: Value = read_body_json(resp).await;
    let token = body["access_token"].as_str().unwrap().to_string();
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["expires_in"], TEST_EXPIRY_SECS as i64);
    assert!(token.starts_with("eyJ"), "expected a JWT, got {token}");

    // 3. /auth/me
    let resp = call_service(
        &app,
        TestRequest::get()
            .uri("/auth/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200, "/me expected 200");
    let body: Value = read_body_json(resp).await;
    assert_eq!(body["email"], "alice@example.com");
    assert_eq!(body["status"], "pending");
    assert_eq!(body["kyc_level"], "none");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn duplicate_email_returns_409(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let req1 = TestRequest::post()
        .uri("/auth/register")
        .set_json(json!({"email": "dup@example.com", "password": "validpassword"}))
        .to_request();
    assert_eq!(call_service(&app, req1).await.status(), 201);

    let req2 = TestRequest::post()
        .uri("/auth/register")
        .set_json(json!({"email": "dup@example.com", "password": "anothervalidone"}))
        .to_request();
    let resp = call_service(&app, req2).await;
    assert_eq!(resp.status(), 409);
    let body: Value = read_body_json(resp).await;
    assert_eq!(body["code"], "CONFLICT");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn login_wrong_password_returns_401(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let req = TestRequest::post()
        .uri("/auth/register")
        .set_json(json!({"email": "bob@example.com", "password": "rightpassword"}))
        .to_request();
    let _ = call_service(&app, req).await;

    let req = TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({"email": "bob@example.com", "password": "wrongpassword"}))
        .to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    let body: Value = read_body_json(resp).await;
    assert_eq!(body["code"], "UNAUTHORIZED");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn login_unknown_email_returns_401(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let req = TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({"email": "ghost@example.com", "password": "irrelevant"}))
        .to_request();
    assert_eq!(call_service(&app, req).await.status(), 401);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn me_without_authorization_returns_401(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let req = TestRequest::get().uri("/auth/me").to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    let body: Value = read_body_json(resp).await;
    assert_eq!(body["code"], "UNAUTHORIZED");
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn me_with_invalid_token_returns_401(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let req = TestRequest::get()
        .uri("/auth/me")
        .insert_header(("Authorization", "Bearer not.a.real.jwt"))
        .to_request();
    assert_eq!(call_service(&app, req).await.status(), 401);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn me_with_token_from_different_secret_returns_401(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let alien = JwtIssuer::new("a-completely-different-secret-value", 60);
    let (token, _) = alien.issue(uuid::Uuid::new_v4()).unwrap();

    let req = TestRequest::get()
        .uri("/auth/me")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(call_service(&app, req).await.status(), 401);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn email_is_case_insensitive_end_to_end(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let req = TestRequest::post()
        .uri("/auth/register")
        .set_json(json!({"email": "CamelCase@Example.COM", "password": "validpassword"}))
        .to_request();
    assert_eq!(call_service(&app, req).await.status(), 201);

    let req = TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({"email": "camelcase@example.com", "password": "validpassword"}))
        .to_request();
    assert_eq!(call_service(&app, req).await.status(), 200);

    let req = TestRequest::post()
        .uri("/auth/register")
        .set_json(json!({"email": "CAMELCASE@EXAMPLE.com", "password": "differentpw"}))
        .to_request();
    assert_eq!(call_service(&app, req).await.status(), 409);
}

#[sqlx::test(migrations = "../../migrations/postgres")]
async fn invalid_email_format_returns_400(pool: PgPool) {
    let app = init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(issuer()))
            .configure(configure),
    )
    .await;

    let req = TestRequest::post()
        .uri("/auth/register")
        .set_json(json!({"email": "not-an-email", "password": "validpassword"}))
        .to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = read_body_json(resp).await;
    assert_eq!(body["code"], "VALIDATION_ERROR");
    assert!(body["details"].is_object(), "expected per-field details");
}
