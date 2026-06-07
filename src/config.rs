use std::path::Path;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePreference {
    Auto,
    Podman,
    Docker,
    HostNative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub scheduler: SchedulerConfig,
    pub storage: StorageConfig,
    pub security: SecurityConfig,
    pub runtime: RuntimeConfig,
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub concurrency: usize,
    pub default_timeout_secs: u64,
    pub shutdown_grace_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub database_path: Utf8PathBuf,
    pub workspace_root: Utf8PathBuf,
    pub artifact_root: Utf8PathBuf,
    pub log_root: Utf8PathBuf,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub source_allowlist: Vec<Utf8PathBuf>,
    pub denied_env: Vec<String>,
    pub max_log_read_bytes: usize,
    pub enable_host_native: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub preference: RuntimePreference,
    pub default_image: String,
    pub network_enabled: bool,
    pub memory_limit: Option<String>,
    pub cpu_limit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub stdio_enabled: bool,
    pub control_socket: Utf8PathBuf,
}

impl Config {
    pub fn for_test_with_allowlist(source_allowlist: Vec<Utf8PathBuf>) -> Self {
        Self {
            scheduler: SchedulerConfig {
                concurrency: 1,
                default_timeout_secs: 3600,
                shutdown_grace_secs: 10,
            },
            storage: StorageConfig {
                database_path: "/tmp/kernel-builder/kbs.db".into(),
                workspace_root: "/tmp/kernel-builder/workspaces".into(),
                artifact_root: "/tmp/kernel-builder/artifacts".into(),
                log_root: "/tmp/kernel-builder/logs".into(),
                retention_days: 14,
            },
            security: SecurityConfig {
                source_allowlist,
                denied_env: vec![
                    "LD_PRELOAD".into(),
                    "LD_LIBRARY_PATH".into(),
                    "SSH_AUTH_SOCK".into(),
                    "GIT_ASKPASS".into(),
                ],
                max_log_read_bytes: 64 * 1024,
                enable_host_native: false,
            },
            runtime: RuntimeConfig {
                preference: RuntimePreference::Auto,
                default_image: "ghcr.io/kernel-builder/linux-build:latest".into(),
                network_enabled: false,
                memory_limit: None,
                cpu_limit: None,
            },
            mcp: McpConfig {
                stdio_enabled: true,
                control_socket: "/tmp/kernel-builder/kbs.sock".into(),
            },
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.scheduler.concurrency == 0 {
            return Err(Error::Config("concurrency must be at least 1".into()));
        }
        if self.security.source_allowlist.is_empty() {
            return Err(Error::Config("source allowlist must not be empty".into()));
        }
        if self.runtime.preference == RuntimePreference::HostNative
            && !self.security.enable_host_native
        {
            return Err(Error::Config(
                "host-native runtime requires enable_host_native=true".into(),
            ));
        }
        if self.security.max_log_read_bytes == 0 {
            return Err(Error::Config("max_log_read_bytes must be positive".into()));
        }
        Ok(())
    }
}

pub fn load_config(path: impl AsRef<Path>) -> Result<Config> {
    let text = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&text)?;
    config.validate()?;
    Ok(config)
}
