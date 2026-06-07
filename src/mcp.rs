use camino::Utf8PathBuf;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::{BuildRequest, EnvVar};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleKernelBuildInput {
    #[schemars(with = "String")]
    pub source_root: Utf8PathBuf,
    pub git_ref: Option<String>,
    pub profile: Option<String>,
    pub arch: String,
    pub config_target: String,
    #[schemars(with = "Vec<String>")]
    pub config_fragments: Vec<Utf8PathBuf>,
    pub make_targets: Vec<String>,
    pub env: Vec<EnvVar>,
    pub timeout_secs: Option<u64>,
    pub priority: Option<i64>,
    pub artifact_patterns: Vec<String>,
}

impl From<ScheduleKernelBuildInput> for BuildRequest {
    fn from(value: ScheduleKernelBuildInput) -> Self {
        Self {
            source_root: value.source_root,
            git_ref: value.git_ref,
            profile: value.profile,
            arch: value.arch,
            config_target: value.config_target,
            config_fragments: value.config_fragments,
            make_targets: value.make_targets,
            env: value.env,
            timeout_secs: value.timeout_secs,
            priority: value.priority.unwrap_or(0),
            artifact_patterns: value.artifact_patterns,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetBuildStatusInput {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CancelBuildInput {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TailBuildLogInput {
    pub job_id: String,
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListArtifactsInput {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetArtifactManifestInput {
    pub job_id: String,
}

#[derive(Clone)]
pub struct KernelBuilderMcp {
    tool_router: ToolRouter<Self>,
}

impl KernelBuilderMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for KernelBuilderMcp {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl KernelBuilderMcp {
    #[tool(description = "Schedule a Linux kernel build")]
    pub async fn schedule_kernel_build(
        &self,
        Parameters(_input): Parameters<ScheduleKernelBuildInput>,
    ) -> String {
        "control socket required".into()
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for KernelBuilderMcp {}

pub async fn serve_stdio() -> anyhow::Result<()> {
    let service = KernelBuilderMcp::new()
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
