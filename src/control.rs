use serde::{Deserialize, Serialize};

use crate::model::{ArtifactRecord, BuildRequest, JobId, JobRecord};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlRequest {
    Status,
    Schedule { request: BuildRequest },
    GetJob { id: JobId },
    ListJobs,
    Cancel { id: JobId },
    TailLog { id: JobId, max_bytes: usize },
    ListArtifacts { id: JobId },
    GetArtifactManifest { id: JobId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlResponse {
    Status {
        queued_jobs: u64,
        active_jobs: u64,
        runtime: String,
    },
    Scheduled {
        id: JobId,
    },
    Job {
        job: JobRecord,
    },
    Jobs {
        jobs: Vec<JobRecord>,
    },
    Canceled {
        id: JobId,
    },
    Log {
        text: String,
        truncated: bool,
    },
    Artifacts {
        artifacts: Vec<ArtifactRecord>,
    },
    ArtifactManifest {
        json: serde_json::Value,
    },
    Error {
        message: String,
    },
}
