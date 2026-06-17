use nexium_matching_engine::{EngineCommand, OrderBookSnapshot};
use sqlx::PgPool;
use tokio::sync::mpsc;

const SNAPSHOT_INTERVAL_SECS: u64 = 5;

pub async fn run(pg_pool: PgPool, ts_pool: PgPool, engine_tx: mpsc::Sender<EngineCommand>) {
    let mut interval =
        tokio::time::interval(std::time::Duration::from_secs(SNAPSHOT_INTERVAL_SECS));

    loop {
        interval.tick().await;

        let pairs = match active_pairs(&pg_pool).await {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(error = %err, "failed to load active pairs for snapshot");
                continue;
            }
        };

        for pair in pairs {
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            if engine_tx
                .send(EngineCommand::GetOrderBook {
                    pair: pair.clone(),
                    reply: reply_tx,
                })
                .await
                .is_err()
            {
                tracing::warn!("engine channel closed; stopping snapshot writer");
                return;
            }

            let snapshot = match reply_rx.await {
                Ok(s) => s,
                Err(_) => continue,
            };

            if let Err(err) = write_snapshot(&ts_pool, &snapshot).await {
                tracing::warn!(error = %err, pair = %pair, "failed to write orderbook snapshot");
            }
        }
    }
}

async fn active_pairs(pool: &PgPool) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT symbol FROM trading.pairs WHERE is_active = true")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

async fn write_snapshot(ts_pool: &PgPool, snap: &OrderBookSnapshot) -> anyhow::Result<()> {
    let bids_json = serde_json::to_value(
        snap.bids
            .iter()
            .map(|(p, q)| (p.to_string(), q.to_string()))
            .collect::<Vec<_>>(),
    )?;
    let asks_json = serde_json::to_value(
        snap.asks
            .iter()
            .map(|(p, q)| (p.to_string(), q.to_string()))
            .collect::<Vec<_>>(),
    )?;

    sqlx::query(
        r#"
        INSERT INTO market.order_book_snapshots (pair, bids, asks, captured_at)
        VALUES ($1, $2, $3, NOW())
        "#,
    )
    .bind(&snap.pair)
    .bind(bids_json)
    .bind(asks_json)
    .execute(ts_pool)
    .await?;

    Ok(())
}
