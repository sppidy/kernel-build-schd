use async_trait::async_trait;
use camino::Utf8PathBuf;
use std::{fs, process::Stdio, sync::Arc};
use tokio::{io::AsyncWriteExt, process::Command};

use crate::{
    config::{Config, RuntimeConfig, RuntimePreference},
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

pub fn container_command_args(
    kind: RuntimeKind,
    image: &str,
    source_root: &str,
    output_root: &str,
    log_path: &str,
    build_command: &[String],
    network_enabled: bool,
) -> Result<Vec<String>> {
    match kind {
        RuntimeKind::Podman | RuntimeKind::Docker => {
            let mut args = vec!["run".into(), "--rm".into()];
            if !network_enabled {
                args.push("--network=none".into());
            }
            args.extend([
                "-v".into(),
                format!("{source_root}:/src:ro"),
                "-v".into(),
                format!("{output_root}:/out:rw"),
                "-v".into(),
                format!("{log_path}:/logs/job.log:rw"),
                "-w".into(),
                "/src".into(),
                image.into(),
            ]);
            args.extend(build_command.iter().cloned());
            Ok(args)
        }
        RuntimeKind::HostNative => Err(Error::Runtime(
            "container args are not valid for host-native runtime".into(),
        )),
    }
}

pub struct HostNativeRuntime;

#[async_trait]
impl BuildRuntime for HostNativeRuntime {
    async fn run(&self, _job_id: JobId, command: RuntimeCommand) -> Result<RuntimeExit> {
        if let Some(parent) = std::path::Path::new(command.log_path.as_str()).parent() {
            fs::create_dir_all(parent)?;
        }
        let output = Command::new(&command.program)
            .args(&command.args)
            .current_dir(command.workspace.as_str())
            .stdin(Stdio::null())
            .output()
            .await?;
        let mut log = tokio::fs::File::create(command.log_path.as_str()).await?;
        log.write_all(&output.stdout).await?;
        log.write_all(&output.stderr).await?;
        log.flush().await?;
        Ok(RuntimeExit {
            code: output.status.code(),
            canceled: false,
        })
    }

    async fn cancel(&self, _job_id: JobId) -> Result<()> {
        Ok(())
    }
}

pub struct OciRuntime {
    pub kind: RuntimeKind,
    pub image: String,
    pub network_enabled: bool,
}

#[async_trait]
impl BuildRuntime for OciRuntime {
    async fn run(&self, _job_id: JobId, command: RuntimeCommand) -> Result<RuntimeExit> {
        let runtime_program = match self.kind {
            RuntimeKind::Podman => "podman",
            RuntimeKind::Docker => "docker",
            RuntimeKind::HostNative => {
                return Err(Error::Runtime("invalid OCI runtime kind".into()));
            }
        };
        if let Some(parent) = std::path::Path::new(command.log_path.as_str()).parent() {
            fs::create_dir_all(parent)?;
        }
        let build_command = std::iter::once(command.program.clone())
            .chain(command.args.clone())
            .collect::<Vec<_>>();
        let args = container_command_args(
            self.kind,
            &self.image,
            command.workspace.as_str(),
            command.workspace.as_str(),
            command.log_path.as_str(),
            &build_command,
            self.network_enabled,
        )?;
        let output = Command::new(runtime_program).args(args).output().await?;
        let mut log = tokio::fs::File::create(command.log_path.as_str()).await?;
        log.write_all(&output.stdout).await?;
        log.write_all(&output.stderr).await?;
        log.flush().await?;
        Ok(RuntimeExit {
            code: output.status.code(),
            canceled: false,
        })
    }

    async fn cancel(&self, _job_id: JobId) -> Result<()> {
        Ok(())
    }
}

pub fn runtime_from_config(config: &Config) -> Result<Arc<dyn BuildRuntime>> {
    let kind = detect_runtime(&config.runtime, |program| {
        std::process::Command::new(program)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })?;

    match kind {
        RuntimeKind::HostNative => Ok(Arc::new(HostNativeRuntime)),
        RuntimeKind::Podman | RuntimeKind::Docker => Ok(Arc::new(OciRuntime {
            kind,
            image: config.runtime.default_image.clone(),
            network_enabled: config.runtime.network_enabled,
        })),
    }
}
