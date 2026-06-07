use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

use crate::error::{Error, Result};
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

#[async_trait]
pub trait ControlHandler: Send + Sync + 'static {
    async fn handle(&self, request: ControlRequest) -> Result<ControlResponse>;
}

pub struct ControlClient {
    socket: PathBuf,
}

impl ControlClient {
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self> {
        let socket = path.as_ref().to_path_buf();
        for _ in 0..50 {
            if fs::try_exists(&socket).await? {
                return Ok(Self { socket });
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Err(Error::Runtime(format!(
            "control socket {} is unavailable",
            socket.display()
        )))
    }

    pub async fn request(&self, request: ControlRequest) -> Result<ControlResponse> {
        let mut stream = UnixStream::connect(&self.socket).await?;
        let payload = serde_json::to_vec(&request)?;
        stream.write_all(&payload).await?;
        stream.write_all(b"\n").await?;
        stream.shutdown().await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        if line.is_empty() {
            return Err(Error::Runtime("empty control response".into()));
        }
        Ok(serde_json::from_str(&line)?)
    }
}

pub async fn serve_control_socket(
    path: impl AsRef<Path>,
    handler: Arc<dyn ControlHandler>,
) -> Result<()> {
    let path = path.as_ref().to_path_buf();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    if fs::try_exists(&path).await? {
        fs::remove_file(&path).await?;
    }
    let listener = UnixListener::bind(&path)?;

    loop {
        let (stream, _) = listener.accept().await?;
        let handler = Arc::clone(&handler);
        tokio::spawn(async move {
            if let Err(err) = handle_stream(stream, handler).await {
                tracing::warn!(error = %err, "control request failed");
            }
        });
    }
}

async fn handle_stream(stream: UnixStream, handler: Arc<dyn ControlHandler>) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let request: ControlRequest = serde_json::from_str(&line)?;
    let response = match handler.handle(request).await {
        Ok(value) => value,
        Err(err) => ControlResponse::Error {
            message: err.to_string(),
        },
    };
    let mut stream = reader.into_inner();
    stream
        .write_all(serde_json::to_string(&response)?.as_bytes())
        .await?;
    stream.write_all(b"\n").await?;
    Ok(())
}
