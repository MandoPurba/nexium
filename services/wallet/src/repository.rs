//! Persistence layer for `wallet.wallets` and `wallet.wallet_txns`.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

/// Currencies provisioned for every newly registered user.
const DEFAULT_CURRENCIES: [&str; 3] = ["BTC", "ETH", "USDT"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WalletRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub currency: String,
    pub balance: Decimal,
    pub locked_balance: Decimal,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct WalletTxnRecord {
    pub id: Uuid,
    pub wallet_id: Uuid,
    pub amount: Decimal,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum DepositError {
    #[error("wallet not found")]
    NotFound,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("insufficient balance")]
    InsufficientBalance,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// Create the default wallet set (BTC, ETH, USDT) for a newly registered
/// user. Idempotent via `ON CONFLICT DO NOTHING` — safe to call more than
/// once for the same user.
pub async fn create_default_wallets(pool: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    for currency in DEFAULT_CURRENCIES {
        sqlx::query(
            r#"
            INSERT INTO wallet.wallets (user_id, currency)
            VALUES ($1, $2)
            ON CONFLICT (user_id, currency) DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(currency)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn find_by_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<WalletRecord>, sqlx::Error> {
    sqlx::query_as::<_, WalletRecord>(
        r#"
        SELECT id, user_id, currency, balance, locked_balance, updated_at
        FROM wallet.wallets
        WHERE user_id = $1
        ORDER BY currency
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn find_by_currency(
    pool: &PgPool,
    user_id: Uuid,
    currency: &str,
) -> Result<Option<WalletRecord>, sqlx::Error> {
    sqlx::query_as::<_, WalletRecord>(
        r#"
        SELECT id, user_id, currency, balance, locked_balance, updated_at
        FROM wallet.wallets
        WHERE user_id = $1 AND currency = $2
        "#,
    )
    .bind(user_id)
    .bind(currency)
    .fetch_optional(pool)
    .await
}

/// Simulate a deposit: credit `amount` to the user's `currency` wallet and
/// record a confirmed `wallet_txns` entry, atomically.
pub async fn deposit(
    pool: &PgPool,
    user_id: Uuid,
    currency: &str,
    amount: Decimal,
) -> Result<WalletTxnRecord, DepositError> {
    let mut tx = pool.begin().await?;

    let wallet_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM wallet.wallets WHERE user_id = $1 AND currency = $2",
    )
    .bind(user_id)
    .bind(currency)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DepositError::NotFound)?;

    let txn = sqlx::query_as::<_, WalletTxnRecord>(
        r#"
        INSERT INTO wallet.wallet_txns (wallet_id, type, amount, status)
        VALUES ($1, 'deposit', $2, 'confirmed')
        RETURNING id, wallet_id, amount, status::text AS status, created_at
        "#,
    )
    .bind(wallet_id)
    .bind(amount)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE wallet.wallets SET balance = balance + $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(amount)
    .bind(wallet_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(txn)
}

/// Move `amount` from `balance` to `locked_balance`. The balance check and
/// the update happen in a single statement, so concurrent lock attempts
/// can't both succeed against the same funds.
pub async fn lock_balance(
    pool: &PgPool,
    user_id: Uuid,
    currency: &str,
    amount: Decimal,
) -> Result<(), LockError> {
    let result = sqlx::query(
        r#"
        UPDATE wallet.wallets
        SET balance = balance - $1,
            locked_balance = locked_balance + $1,
            updated_at = NOW()
        WHERE user_id = $2 AND currency = $3 AND balance >= $1
        "#,
    )
    .bind(amount)
    .bind(user_id)
    .bind(currency)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(LockError::InsufficientBalance);
    }
    Ok(())
}

/// Move `amount` from `locked_balance` back to `balance`.
pub async fn unlock_balance(
    pool: &PgPool,
    user_id: Uuid,
    currency: &str,
    amount: Decimal,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE wallet.wallets
        SET balance = balance + $1,
            locked_balance = locked_balance - $1,
            updated_at = NOW()
        WHERE user_id = $2 AND currency = $3
        "#,
    )
    .bind(amount)
    .bind(user_id)
    .bind(currency)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations/postgres")]
    async fn lock_balance_moves_funds_to_locked(pool: PgPool) {
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO auth.users (id, email, password_hash) VALUES ($1, $2, 'hash')")
            .bind(user_id)
            .bind(format!("{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();
        create_default_wallets(&pool, user_id).await.unwrap();
        deposit(&pool, user_id, "USDT", Decimal::from(1000))
            .await
            .unwrap();

        lock_balance(&pool, user_id, "USDT", Decimal::from(400))
            .await
            .unwrap();

        let wallet = find_by_currency(&pool, user_id, "USDT")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(wallet.balance, Decimal::from(600));
        assert_eq!(wallet.locked_balance, Decimal::from(400));
    }

    #[sqlx::test(migrations = "../../migrations/postgres")]
    async fn lock_balance_insufficient_returns_error(pool: PgPool) {
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO auth.users (id, email, password_hash) VALUES ($1, $2, 'hash')")
            .bind(user_id)
            .bind(format!("{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();
        create_default_wallets(&pool, user_id).await.unwrap();
        deposit(&pool, user_id, "USDT", Decimal::from(100))
            .await
            .unwrap();

        let err = lock_balance(&pool, user_id, "USDT", Decimal::from(500))
            .await
            .unwrap_err();
        assert!(matches!(err, LockError::InsufficientBalance));

        // Balance must be untouched after a failed lock.
        let wallet = find_by_currency(&pool, user_id, "USDT")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(wallet.balance, Decimal::from(100));
        assert_eq!(wallet.locked_balance, Decimal::ZERO);
    }

    #[sqlx::test(migrations = "../../migrations/postgres")]
    async fn unlock_balance_returns_funds_to_available(pool: PgPool) {
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO auth.users (id, email, password_hash) VALUES ($1, $2, 'hash')")
            .bind(user_id)
            .bind(format!("{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();
        create_default_wallets(&pool, user_id).await.unwrap();
        deposit(&pool, user_id, "USDT", Decimal::from(1000))
            .await
            .unwrap();
        lock_balance(&pool, user_id, "USDT", Decimal::from(400))
            .await
            .unwrap();

        unlock_balance(&pool, user_id, "USDT", Decimal::from(150))
            .await
            .unwrap();

        let wallet = find_by_currency(&pool, user_id, "USDT")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(wallet.balance, Decimal::from(750));
        assert_eq!(wallet.locked_balance, Decimal::from(250));
    }

    #[sqlx::test(migrations = "../../migrations/postgres")]
    async fn create_default_wallets_is_idempotent(pool: PgPool) {
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO auth.users (id, email, password_hash) VALUES ($1, $2, 'hash')")
            .bind(user_id)
            .bind(format!("{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();

        create_default_wallets(&pool, user_id).await.unwrap();
        create_default_wallets(&pool, user_id).await.unwrap();

        let wallets = find_by_user(&pool, user_id).await.unwrap();
        assert_eq!(wallets.len(), DEFAULT_CURRENCIES.len());
    }
}
