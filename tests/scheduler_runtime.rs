use kernel_builder::config::{RuntimeConfig, RuntimePreference};
use kernel_builder::runtime::{
    container_command_args, detect_runtime, HostNativeRuntime, RuntimeKind,
};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use kernel_builder::config::Config;
use kernel_builder::db::Store;
use kernel_builder::model::{BuildRequest, JobId, JobState};
use kernel_builder::runtime::{BuildRuntime, RuntimeCommand, RuntimeExit};
use kernel_builder::scheduler::Scheduler;

#[test]
fn explicit_host_native_is_selected_only_when_requested() {
    let config = RuntimeConfig {
        preference: RuntimePreference::HostNative,
        default_image: "unused".into(),
        network_enabled: false,
        memory_limit: None,
        cpu_limit: None,
    };

    let runtime = detect_runtime(&config, |_program| false).unwrap();

    assert_eq!(runtime, RuntimeKind::HostNative);
}

#[test]
fn auto_prefers_podman_before_docker() {
    let config = RuntimeConfig {
        preference: RuntimePreference::Auto,
        default_image: "image".into(),
        network_enabled: false,
        memory_limit: None,
        cpu_limit: None,
    };

    let runtime =
        detect_runtime(&config, |program| matches!(program, "podman" | "docker")).unwrap();

    assert_eq!(runtime, RuntimeKind::Podman);
}

#[derive(Default)]
struct RecordingRuntime {
    jobs: Arc<Mutex<Vec<JobId>>>,
}

#[async_trait]
impl BuildRuntime for RecordingRuntime {
    async fn run(
        &self,
        job_id: JobId,
        _command: RuntimeCommand,
    ) -> kernel_builder::error::Result<RuntimeExit> {
        self.jobs.lock().unwrap().push(job_id);
        Ok(RuntimeExit {
            code: Some(0),
            canceled: false,
        })
    }

    async fn cancel(&self, _job_id: JobId) -> kernel_builder::error::Result<()> {
        Ok(())
    }
}

fn scheduler_request() -> BuildRequest {
    BuildRequest {
        source_root: "/allowed/linux".into(),
        git_ref: None,
        profile: None,
        arch: "x86_64".into(),
        config_target: "defconfig".into(),
        config_fragments: vec![],
        make_targets: vec!["bzImage".into()],
        env: vec![],
        timeout_secs: Some(60),
        priority: 0,
        artifact_patterns: vec![],
    }
}

#[tokio::test]
async fn scheduler_runs_queued_job_to_success() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("kbs.db")).unwrap();
    let job = store.enqueue(scheduler_request()).unwrap();
    let runtime = Arc::new(RecordingRuntime::default());
    let config = Config::for_test_with_allowlist(vec!["/allowed".into()]);
    let scheduler = Scheduler::new(config, store, runtime);

    scheduler.run_one_queued_job().await.unwrap();

    assert_eq!(
        scheduler.get_job(job.id).unwrap().state,
        JobState::Succeeded
    );
}

#[test]
fn container_command_disables_network_by_default() {
    let args = container_command_args(
        RuntimeKind::Podman,
        "image:latest",
        "/src",
        "/out",
        "/logs/job.log",
        &["make".into(), "ARCH=arm64".into(), "Image".into()],
        false,
    )
    .unwrap();

    assert!(args.contains(&"--network=none".into()));
    assert!(!args.iter().any(|arg| arg.contains("docker.sock")));
}

#[tokio::test]
async fn host_native_runtime_runs_command_and_writes_log() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("job.log");
    let runtime = HostNativeRuntime;
    let command = RuntimeCommand {
        program: "sh".into(),
        args: vec!["-c".into(), "printf hello".into()],
        workspace: dir.path().to_string_lossy().to_string().into(),
        log_path: log_path.to_string_lossy().to_string().into(),
    };

    let exit = runtime.run(JobId::new(), command).await.unwrap();

    assert_eq!(exit.code, Some(0));
    assert_eq!(std::fs::read_to_string(log_path).unwrap(), "hello");
}
