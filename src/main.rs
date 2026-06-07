use kernel_builder::cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::run().await
}
