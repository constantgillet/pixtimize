#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pixtimize::app::run().await
}
