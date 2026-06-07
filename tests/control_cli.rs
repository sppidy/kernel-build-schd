use kernel_builder::control::{ControlRequest, ControlResponse};
use kernel_builder::daemon::DaemonHealth;
use std::sync::Arc;

use async_trait::async_trait;
use kernel_builder::config::Config;
use kernel_builder::control::{serve_control_socket, ControlClient, ControlHandler};
use kernel_builder::daemon::DaemonControl;
use kernel_builder::db::Store;
use kernel_builder::error::Result;
use kernel_builder::model::BuildRequest;

#[test]
fn control_messages_round_trip_json() {
    let request = ControlRequest::Status;
    let json = serde_json::to_string(&request).unwrap();
    let decoded: ControlRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, ControlRequest::Status);
}

#[test]
fn status_response_has_queue_counts() {
    let response = ControlResponse::Status {
        queued_jobs: 2,
        active_jobs: 1,
        runtime: "podman".into(),
    };

    let json = serde_json::to_string(&response).unwrap();

    assert!(json.contains("queued_jobs"));
}

#[test]
fn daemon_health_reports_runtime_and_database() {
    let health = DaemonHealth {
        database_ok: true,
        runtime: "fake".into(),
        queued_jobs: 0,
        active_jobs: 0,
    };

    assert!(health.database_ok);
    assert_eq!(health.runtime, "fake");
}

struct StatusHandler;

#[async_trait]
impl ControlHandler for StatusHandler {
    async fn handle(&self, request: ControlRequest) -> Result<ControlResponse> {
        match request {
            ControlRequest::Status => Ok(ControlResponse::Status {
                queued_jobs: 0,
                active_jobs: 0,
                runtime: "fake".into(),
            }),
            _ => Ok(ControlResponse::Error {
                message: "unsupported in test".into(),
            }),
        }
    }
}

#[tokio::test]
async fn control_client_round_trips_status_over_unix_socket() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("kbs.sock");
    let server = tokio::spawn(serve_control_socket(
        socket.clone(),
        Arc::new(StatusHandler),
    ));

    let client = ControlClient::connect(&socket).await.unwrap();
    let response = client.request(ControlRequest::Status).await.unwrap();

    server.abort();

    assert_eq!(
        response,
        ControlResponse::Status {
            queued_jobs: 0,
            active_jobs: 0,
            runtime: "fake".into(),
        }
    );
}

#[tokio::test]
async fn daemon_control_schedules_and_returns_job() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::for_test_with_allowlist(vec!["/allowed".into()]);
    config.storage.database_path = dir
        .path()
        .join("kbs.db")
        .to_string_lossy()
        .to_string()
        .into();
    let store = Store::open(config.storage.database_path.as_std_path()).unwrap();
    let control = DaemonControl::new_for_test(config, store);

    let response = control
        .handle(ControlRequest::Schedule {
            request: BuildRequest {
                source_root: "/allowed/linux".into(),
                git_ref: None,
                profile: None,
                arch: "x86_64".into(),
                config_target: "defconfig".into(),
                config_fragments: vec![],
                make_targets: vec!["bzImage".into()],
                env: vec![],
                timeout_secs: None,
                priority: 0,
                artifact_patterns: vec![],
            },
        })
        .await
        .unwrap();

    assert!(matches!(response, ControlResponse::Scheduled { .. }));
}
