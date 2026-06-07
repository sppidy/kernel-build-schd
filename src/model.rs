use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::{Error, Result};

pub use crate::ids::JobId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Preparing,
    Running,
    Collecting,
    Succeeded,
    Canceling,
    Canceled,
    Failed,
    TimedOut,
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobState::Succeeded | JobState::Canceled | JobState::Failed | JobState::TimedOut
        )
    }

    pub fn transition_to(self, next: JobState) -> Result<JobState> {
        if self.is_terminal() {
            return Err(Error::InvalidState(format!(
                "cannot transition from {self:?} to {next:?}"
            )));
        }
        Ok(next)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BuildRequest {
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
    pub priority: i64,
    pub artifact_patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: JobId,
    pub request: BuildRequest,
    pub state: JobState,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub failure: Option<FailureInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FailureInfo {
    pub category: FailureCategory,
    pub message: String,
    pub phase: String,
    pub exit_code: Option<i32>,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    InvalidRequest,
    PolicyDenied,
    SourceUnavailable,
    GitFailure,
    RuntimeUnavailable,
    ImageUnavailable,
    PrepareFailed,
    BuildFailed,
    ArtifactCollectionFailed,
    TimedOut,
    Canceled,
    InternalError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactRecord {
    pub job_id: JobId,
    #[schemars(with = "String")]
    pub path: Utf8PathBuf,
    pub kind: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SchedulerHealth {
    pub version: String,
    pub database_ok: bool,
    pub runtime: String,
    pub queued_jobs: u64,
    pub active_jobs: u64,
}
