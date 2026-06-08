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

#[derive(Debug, Clone, Copy)]
pub struct ContainerCommandSpec<'a> {
    pub kind: RuntimeKind,
    pub image: &'a str,
    pub source_root: &'a str,
    pub output_root: &'a str,
    pub log_path: &'a str,
    pub build_command: &'a [String],
    pub network_enabled: bool,
    pub memory_limit: Option<&'a str>,
    pub cpu_limit: Option<&'a str>,
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

pub fn container_command_args(spec: ContainerCommandSpec<'_>) -> Result<Vec<String>> {
    match spec.kind {
        RuntimeKind::Podman | RuntimeKind::Docker => {
            let mut args = vec!["run".into(), "--rm".into()];
            if !spec.network_enabled {
                args.push("--network=none".into());
            }
            if let Some(limit) = spec.memory_limit {
                args.push(format!("--memory={limit}"));
            }
            if let Some(limit) = spec.cpu_limit {
                args.push(format!("--cpus={limit}"));
            }
            args.extend([
                "-v".into(),
                format!("{}:/src:ro", spec.source_root),
                "-v".into(),
                format!("{}:/out:rw", spec.output_root),
                "-v".into(),
                format!("{}:/logs/job.log:rw", spec.log_path),
                "-w".into(),
                "/src".into(),
                spec.image.into(),
            ]);
            args.extend(spec.build_command.iter().cloned());
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
    pub memory_limit: Option<String>,
    pub cpu_limit: Option<String>,
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
        let args = container_command_args(ContainerCommandSpec {
            kind: self.kind,
            image: &self.image,
            source_root: command.workspace.as_str(),
            output_root: command.workspace.as_str(),
            log_path: command.log_path.as_str(),
            build_command: &build_command,
            network_enabled: self.network_enabled,
            memory_limit: self.memory_limit.as_deref(),
            cpu_limit: self.cpu_limit.as_deref(),
        })?;
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
            memory_limit: config.runtime.memory_limit.clone(),
            cpu_limit: config.runtime.cpu_limit.clone(),
        })),
    }
}
