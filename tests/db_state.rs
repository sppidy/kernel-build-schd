use kernel_builder::model::{JobId, JobState};

#[test]
fn terminal_jobs_do_not_transition_back_to_running() {
    let result = JobState::Succeeded.transition_to(JobState::Running);
    assert!(result.is_err());
}

#[test]
fn generated_job_ids_are_prefixed_and_parseable() {
    let id = JobId::new();
    let encoded = id.to_string();
    assert!(encoded.starts_with("job_"));
    assert_eq!(encoded.parse::<JobId>().unwrap(), id);
}
