use async_trait::async_trait;
use camino::Utf8PathBuf;

use crate::{
    config::{RuntimeConfig, RuntimePreference},
    error::{Error, Result},
    model::JobId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    Podman,
    Docker,
    HostNative,
}

#[derive(Debug, Clone)]
pub struct RuntimeCommand {
    pub program: String,
    pub args: Vec<String>,
    pub workspace: Utf8PathBuf,
    pub log_path: Utf8PathBuf,
}

#[derive(Debug, Clone)]
pub struct RuntimeExit {
    pub code: Option<i32>,
    pub canceled: bool,
}

#[async_trait]
pub trait BuildRuntime: Send + Sync {
    async fn run(&self, job_id: JobId, command: RuntimeCommand) -> Result<RuntimeExit>;
    async fn cancel(&self, job_id: JobId) -> Result<()>;
}

pub fn detect_runtime(
    config: &RuntimeConfig,
    executable_exists: impl Fn(&str) -> bool,
) -> Result<RuntimeKind> {
    match config.preference {
        RuntimePreference::Podman => executable_exists("podman")
            .then_some(RuntimeKind::Podman)
            .ok_or_else(|| Error::Runtime("podman was requested but not found".into())),
        RuntimePreference::Docker => executable_exists("docker")
            .then_some(RuntimeKind::Docker)
            .ok_or_else(|| Error::Runtime("docker was requested but not found".into())),
        RuntimePreference::HostNative => Ok(RuntimeKind::HostNative),
        RuntimePreference::Auto => {
            if executable_exists("podman") {
                Ok(RuntimeKind::Podman)
            } else if executable_exists("docker") {
                Ok(RuntimeKind::Docker)
            } else {
                Err(Error::Runtime("no usable container runtime found".into()))
            }
        }
    }
}
