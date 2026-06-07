use kernel_builder::control::{ControlRequest, ControlResponse};

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
