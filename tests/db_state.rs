use kernel_builder::model::{JobId, JobState};
use kernel_builder::{
    db::Store,
    model::{BuildRequest, JobState as StoredJobState},
};

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

fn build_request() -> BuildRequest {
    BuildRequest {
        source_root: "/allowed/linux".into(),
        git_ref: Some("HEAD".into()),
        profile: Some("x86_64-defconfig".into()),
        arch: "x86_64".into(),
        config_target: "defconfig".into(),
        config_fragments: vec![],
        make_targets: vec!["bzImage".into()],
        env: vec![],
        timeout_secs: Some(60),
        priority: 0,
        artifact_patterns: vec!["arch/x86/boot/bzImage".into()],
    }
}

#[test]
fn store_persists_queued_job_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("kbs.db");

    let id = {
        let store = Store::open(&db_path).unwrap();
        store.enqueue(build_request()).unwrap().id
    };

    let reopened = Store::open(&db_path).unwrap();
    let job = reopened.get_job(id).unwrap();

    assert_eq!(job.id, id);
    assert_eq!(job.state, StoredJobState::Queued);
}

#[test]
fn store_records_state_transition_events() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("kbs.db")).unwrap();
    let id = store.enqueue(build_request()).unwrap().id;

    store.set_state(id, StoredJobState::Preparing).unwrap();
    store.set_state(id, StoredJobState::Running).unwrap();

    let events = store.events(id).unwrap();
    assert_eq!(events.len(), 3);
    assert!(events[0].contains("queued"));
    assert!(events[2].contains("running"));
}
