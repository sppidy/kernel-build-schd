use camino::Utf8PathBuf;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::control::{ControlClient, ControlRequest, ControlResponse};
use crate::model::{BuildRequest, EnvVar, TreeRegistration};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleKernelBuildInput {
    #[schemars(with = "Option<String>")]
    pub source_root: Option<Utf8PathBuf>,
    pub source_url: Option<String>,
    pub tree_name: Option<String>,
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
            source_url: value.source_url,
            tree_name: value.tree_name,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RegisterSourceTreeInput {
    pub name: String,
    #[schemars(with = "Option<String>")]
    pub source_root: Option<Utf8PathBuf>,
    pub source_url: Option<String>,
    pub default_ref: Option<String>,
}

impl From<RegisterSourceTreeInput> for TreeRegistration {
    fn from(value: RegisterSourceTreeInput) -> Self {
        Self {
            name: value.name,
            source_root: value.source_root,
            source_url: value.source_url,
            default_ref: value.default_ref,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceTreeNameInput {
    pub name: String,
}

#[derive(Clone)]
pub struct KernelBuilderMcp {
    socket: PathBuf,
    tool_router: ToolRouter<Self>,
}

impl KernelBuilderMcp {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
            tool_router: Self::tool_router(),
        }
    }

    async fn call_control(&self, request: ControlRequest) -> String {
        match ControlClient::connect(&self.socket).await {
            Ok(client) => match client.request(request).await {
                Ok(response) => {
                    control_response_to_text(response).unwrap_or_else(|err| format!("error: {err}"))
                }
                Err(err) => format!("error: {err}"),
            },
            Err(err) => format!("error: {err}"),
        }
    }
}

impl Default for KernelBuilderMcp {
    fn default() -> Self {
        Self::new("/tmp/kernel-builder/kbs.sock")
    }
}

pub fn control_response_to_text(response: ControlResponse) -> crate::error::Result<String> {
    match response {
        ControlResponse::Status {
            queued_jobs,
            active_jobs,
            runtime,
        } => Ok(format!(
            "runtime={runtime} queued_jobs={queued_jobs} active_jobs={active_jobs}"
        )),
        ControlResponse::Scheduled { id } => Ok(format!("scheduled {id}")),
        ControlResponse::Job { job } => Ok(serde_json::to_string_pretty(&job)?),
        ControlResponse::Jobs { jobs } => Ok(serde_json::to_string_pretty(&jobs)?),
        ControlResponse::Canceled { id } => Ok(format!("canceled {id}")),
        ControlResponse::Log { text, truncated } => {
            if truncated {
                Ok(format!("{text}\n[truncated]"))
            } else {
                Ok(text)
            }
        }
        ControlResponse::Artifacts { artifacts } => Ok(serde_json::to_string_pretty(&artifacts)?),
        ControlResponse::ArtifactManifest { json } => Ok(serde_json::to_string_pretty(&json)?),
        ControlResponse::TreeRegistered { tree } => Ok(serde_json::to_string_pretty(&tree)?),
        ControlResponse::Tree { tree } => Ok(serde_json::to_string_pretty(&tree)?),
        ControlResponse::Trees { trees } => Ok(serde_json::to_string_pretty(&trees)?),
        ControlResponse::TreeRemoved { name, removed } => Ok(serde_json::to_string_pretty(
            &serde_json::json!({ "name": name, "removed": removed }),
        )?),
        ControlResponse::Error { message } => Ok(format!("error: {message}")),
    }
}

#[tool_router]
impl KernelBuilderMcp {
    #[tool(description = "Schedule a Linux kernel build")]
    pub async fn schedule_kernel_build(
        &self,
        Parameters(input): Parameters<ScheduleKernelBuildInput>,
    ) -> String {
        self.call_control(ControlRequest::Schedule {
            request: Box::new(input.into()),
        })
        .await
    }

    #[tool(description = "Get build status by job id")]
    pub async fn get_build_status(
        &self,
        Parameters(input): Parameters<GetBuildStatusInput>,
    ) -> String {
        match input.job_id.parse() {
            Ok(id) => self.call_control(ControlRequest::GetJob { id }).await,
            Err(err) => format!("error: {err}"),
        }
    }

    #[tool(description = "Cancel a queued or running build")]
    pub async fn cancel_build(&self, Parameters(input): Parameters<CancelBuildInput>) -> String {
        match input.job_id.parse() {
            Ok(id) => self.call_control(ControlRequest::Cancel { id }).await,
            Err(err) => format!("error: {err}"),
        }
    }

    #[tool(description = "Tail a bounded amount of build log output")]
    pub async fn tail_build_log(&self, Parameters(input): Parameters<TailBuildLogInput>) -> String {
        match input.job_id.parse() {
            Ok(id) => {
                self.call_control(ControlRequest::TailLog {
                    id,
                    max_bytes: input.max_bytes.unwrap_or(4096),
                })
                .await
            }
            Err(err) => format!("error: {err}"),
        }
    }

    #[tool(description = "List recent builds")]
    pub async fn list_builds(&self) -> String {
        self.call_control(ControlRequest::ListJobs).await
    }

    #[tool(description = "Register or update a named source tree")]
    pub async fn register_source_tree(
        &self,
        Parameters(input): Parameters<RegisterSourceTreeInput>,
    ) -> String {
        self.call_control(ControlRequest::RegisterTree { tree: input.into() })
            .await
    }

    #[tool(description = "List registered source trees")]
    pub async fn list_source_trees(&self) -> String {
        self.call_control(ControlRequest::ListTrees).await
    }

    #[tool(description = "Get a registered source tree by name")]
    pub async fn get_source_tree(
        &self,
        Parameters(input): Parameters<SourceTreeNameInput>,
    ) -> String {
        self.call_control(ControlRequest::GetTree { name: input.name })
            .await
    }

    #[tool(description = "Remove a registered source tree by name")]
    pub async fn remove_source_tree(
        &self,
        Parameters(input): Parameters<SourceTreeNameInput>,
    ) -> String {
        self.call_control(ControlRequest::RemoveTree { name: input.name })
            .await
    }

    #[tool(description = "List retained artifacts for a build")]
    pub async fn list_artifacts(
        &self,
        Parameters(input): Parameters<ListArtifactsInput>,
    ) -> String {
        match input.job_id.parse() {
            Ok(id) => {
                self.call_control(ControlRequest::ListArtifacts { id })
                    .await
            }
            Err(err) => format!("error: {err}"),
        }
    }

    #[tool(description = "Get the artifact manifest for a build")]
    pub async fn get_artifact_manifest(
        &self,
        Parameters(input): Parameters<GetArtifactManifestInput>,
    ) -> String {
        match input.job_id.parse() {
            Ok(id) => {
                self.call_control(ControlRequest::GetArtifactManifest { id })
                    .await
            }
            Err(err) => format!("error: {err}"),
        }
    }

    #[tool(description = "Get scheduler health")]
    pub async fn get_scheduler_health(&self) -> String {
        self.call_control(ControlRequest::Status).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for KernelBuilderMcp {}

pub async fn serve_stdio(socket: impl Into<PathBuf>) -> anyhow::Result<()> {
    let service = KernelBuilderMcp::new(socket)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
