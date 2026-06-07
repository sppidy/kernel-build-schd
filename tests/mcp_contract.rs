use kernel_builder::mcp::{
    CancelBuildInput, GetArtifactManifestInput, GetBuildStatusInput, ListArtifactsInput,
    ScheduleKernelBuildInput, TailBuildLogInput,
};

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
