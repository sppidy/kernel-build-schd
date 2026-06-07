use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    config::Config,
    control::{serve_control_socket, ControlHandler, ControlRequest, ControlResponse},
    db::Store,
    error::{Error, Result},
    model::JobState,
    policy::Policy,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonHealth {
    pub database_ok: bool,
    pub runtime: String,
    pub queued_jobs: u64,
    pub active_jobs: u64,
}

pub struct DaemonControl {
    config: Config,
    store: Arc<Mutex<Store>>,
}

impl DaemonControl {
    pub fn new_for_test(config: Config, store: Store) -> Self {
        Self {
            config,
            store: Arc::new(Mutex::new(store)),
        }
    }

    fn store(&self) -> Result<MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| Error::Runtime("store mutex poisoned".into()))
    }
}

#[async_trait]
impl ControlHandler for DaemonControl {
    async fn handle(&self, request: ControlRequest) -> Result<ControlResponse> {
        match request {
            ControlRequest::Status => {
                let store = self.store()?;
                Ok(ControlResponse::Status {
                    queued_jobs: store.count_by_state(JobState::Queued)?,
                    active_jobs: store.count_by_state(JobState::Running)?,
                    runtime: format!("{:?}", self.config.runtime.preference).to_lowercase(),
                })
            }
            ControlRequest::Schedule { request } => {
                Policy::new(self.config.security.clone()).validate_request(&request)?;
                let job = self.store()?.enqueue(request)?;
                Ok(ControlResponse::Scheduled { id: job.id })
            }
            ControlRequest::GetJob { id } => Ok(ControlResponse::Job {
                job: self.store()?.get_job(id)?,
            }),
            ControlRequest::ListJobs => Ok(ControlResponse::Jobs {
                jobs: self.store()?.list_jobs()?,
            }),
            ControlRequest::Cancel { id } => {
                self.store()?.set_state(id, JobState::Canceling)?;
                Ok(ControlResponse::Canceled { id })
            }
            ControlRequest::TailLog { id, max_bytes } => {
                let log_path = self.config.storage.log_root.join(format!("{id}.log"));
                let bytes = std::fs::read(log_path.as_std_path()).unwrap_or_default();
                let truncated = bytes.len() > max_bytes;
                let start = bytes.len().saturating_sub(max_bytes);
                let text = String::from_utf8_lossy(&bytes[start..]).to_string();
                Ok(ControlResponse::Log { text, truncated })
            }
            ControlRequest::ListArtifacts { id } => Ok(ControlResponse::Artifacts {
                artifacts: self.store()?.list_artifacts(id)?,
            }),
            ControlRequest::GetArtifactManifest { id } => {
                let artifacts = self.store()?.list_artifacts(id)?;
                Ok(ControlResponse::ArtifactManifest {
                    json: serde_json::json!({
                        "job_id": id.to_string(),
                        "artifacts": artifacts,
                    }),
                })
            }
        }
    }
}

pub async fn run_foreground() -> anyhow::Result<()> {
    anyhow::bail!("daemon startup requires a validated config")
}

pub async fn run_foreground_with_config(config: Config) -> anyhow::Result<()> {
    config.validate()?;
    let store = Store::open(config.storage.database_path.as_std_path())?;
    let control = Arc::new(DaemonControl::new_for_test(config.clone(), store));
    let socket_path = config.mcp.control_socket.as_std_path().to_path_buf();
    let socket = serve_control_socket(socket_path, control);
    let worker = worker_loop(config.clone());
    tokio::pin!(socket);
    tokio::pin!(worker);

    tokio::select! {
        result = &mut socket => {
            result?;
        }
        result = &mut worker => {
            result?;
        }
        _ = tokio::signal::ctrl_c() => {}
    }

    Ok(())
}

async fn worker_loop(config: Config) -> anyhow::Result<()> {
    let runtime = crate::runtime::runtime_from_config(&config)?;
    loop {
        let store = Store::open(config.storage.database_path.as_std_path())?;
        let scheduler = crate::scheduler::Scheduler::new(config.clone(), store, runtime.clone());
        scheduler.run_one_queued_job().await?;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}
