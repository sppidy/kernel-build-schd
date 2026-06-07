use kernel_builder::mcp::{
    control_response_to_text, CancelBuildInput, GetArtifactManifestInput, GetBuildStatusInput,
    ListArtifactsInput, ScheduleKernelBuildInput, TailBuildLogInput,
};
use kernel_builder::{control::ControlResponse, model::JobId};

#[test]
fn schedule_input_schema_contains_source_root_and_arch() {
    let schema = schemars::schema_for!(ScheduleKernelBuildInput);
    let json = serde_json::to_value(schema).unwrap();

    assert!(json.to_string().contains("source_root"));
    assert!(json.to_string().contains("arch"));
}

#[test]
fn bounded_log_input_defaults_are_explicit() {
    let input = TailBuildLogInput {
        job_id: "job_018f03214f1870008000000000000000".into(),
        max_bytes: Some(4096),
    };

    assert_eq!(input.max_bytes, Some(4096));
}

#[test]
fn status_and_cancel_inputs_use_job_id_strings() {
    let status = GetBuildStatusInput {
        job_id: "job_018f03214f1870008000000000000000".into(),
    };
    let cancel = CancelBuildInput {
        job_id: status.job_id.clone(),
    };

    assert_eq!(cancel.job_id, status.job_id);
}

#[test]
fn artifact_inputs_use_job_id_strings() {
    let list = ListArtifactsInput {
        job_id: "job_018f03214f1870008000000000000000".into(),
    };
    let manifest = GetArtifactManifestInput {
        job_id: list.job_id.clone(),
    };

    assert_eq!(manifest.job_id, list.job_id);
}

#[test]
fn mcp_schedule_response_includes_job_id() {
    let id = JobId::new();
    let text = control_response_to_text(ControlResponse::Scheduled { id }).unwrap();

    assert!(text.contains(&id.to_string()));
}

#[test]
fn mcp_error_response_becomes_error_text() {
    let text = control_response_to_text(ControlResponse::Error {
        message: "policy denied".into(),
    })
    .unwrap();

    assert!(text.contains("policy denied"));
}
