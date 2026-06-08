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
use serde_json::json;

#[test]
fn config_file_loads_and_validates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
        [scheduler]
        concurrency = 1
        default_timeout_secs = 60
        shutdown_grace_secs = 5

        [storage]
        database_path = "/tmp/kbs.db"
        workspace_root = "/tmp/kbs-work"
        artifact_root = "/tmp/kbs-artifacts"
        log_root = "/tmp/kbs-logs"
        retention_days = 7

        [security]
        source_allowlist = ["/allowed"]
        denied_env = ["LD_PRELOAD"]
        max_log_read_bytes = 4096
        enable_host_native = false

        [runtime]
        preference = "auto"
        default_image = "image"
        network_enabled = false
        memory_limit = "4g"
        cpu_limit = "2"

        [mcp]
        stdio_enabled = true
        control_socket = "/tmp/kbs.sock"
        "#,
    )
    .unwrap();

    let config = kernel_builder::config::load_config(&path).unwrap();

    assert_eq!(config.scheduler.concurrency, 1);
    assert_eq!(config.security.source_allowlist.len(), 1);
}

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
            request: Box::new(BuildRequest {
                source_root: Some("/allowed/linux".into()),
                source_url: None,
                tree_name: None,
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
            }),
        })
        .await
        .unwrap();

    assert!(matches!(response, ControlResponse::Scheduled { .. }));
}

#[tokio::test]
async fn daemon_control_registers_tree_and_schedules_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::for_test_with_allowlist(vec!["/allowed".into()]);
    config.security.clone_url_allowlist = vec!["https://github.com/spidy/".into()];
    config.storage.database_path = dir
        .path()
        .join("kbs.db")
        .to_string_lossy()
        .to_string()
        .into();
    let store = Store::open(config.storage.database_path.as_std_path()).unwrap();
    let control = DaemonControl::new_for_test(config, store);

    let register: ControlRequest = serde_json::from_value(json!({
        "type": "register_tree",
        "tree": {
            "name": "linux-next",
            "source_url": "https://github.com/spidy/linux-next.git",
            "default_ref": "main"
        }
    }))
    .unwrap();
    let response = control.handle(register).await.unwrap();
    assert!(serde_json::to_string(&response)
        .unwrap()
        .contains("tree_registered"));

    let request: BuildRequest = serde_json::from_value(json!({
        "tree_name": "linux-next",
        "arch": "arm64",
        "config_target": "defconfig",
        "config_fragments": [],
        "make_targets": ["Image"],
        "env": [],
        "priority": 0,
        "artifact_patterns": []
    }))
    .unwrap();
    let response = control
        .handle(ControlRequest::Schedule {
            request: Box::new(request),
        })
        .await
        .unwrap();

    let id = match response {
        ControlResponse::Scheduled { id } => id,
        other => panic!("unexpected response: {other:?}"),
    };
    let response = control.handle(ControlRequest::GetJob { id }).await.unwrap();
    let value = serde_json::to_value(response).unwrap();

    assert_eq!(value["job"]["request"]["tree_name"], "linux-next");
    assert_eq!(
        value["job"]["request"]["source_url"],
        "https://github.com/spidy/linux-next.git"
    );
    assert_eq!(value["job"]["request"]["git_ref"], "main");
}
