#[tokio::main]
async fn main() -> anyhow::Result<()> {
    market_data_service::run().await
}
