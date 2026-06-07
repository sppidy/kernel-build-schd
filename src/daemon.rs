use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonHealth {
    pub database_ok: bool,
    pub runtime: String,
    pub queued_jobs: u64,
    pub active_jobs: u64,
}

pub async fn run_foreground() -> anyhow::Result<()> {
    tracing::info!("kernel builder daemon starting");
    tokio::signal::ctrl_c().await?;
    tracing::info!("kernel builder daemon stopping");
    Ok(())
}
