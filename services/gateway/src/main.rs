#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gateway::run().await
}
