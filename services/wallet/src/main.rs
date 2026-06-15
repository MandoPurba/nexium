#[tokio::main]
async fn main() -> anyhow::Result<()> {
    wallet_service::run().await
}
