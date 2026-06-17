#[tokio::main]
async fn main() -> anyhow::Result<()> {
    order_service::run().await
}
