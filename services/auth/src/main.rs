#[tokio::main]
async fn main() -> anyhow::Result<()> {
    auth_service::run().await
}
