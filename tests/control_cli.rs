use kernel_builder::control::{ControlRequest, ControlResponse};
use kernel_builder::daemon::DaemonHealth;

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
