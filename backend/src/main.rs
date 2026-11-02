#[tokio::main]
async fn main() -> anyhow::Result<()> {
    furfiles::run().await
}
