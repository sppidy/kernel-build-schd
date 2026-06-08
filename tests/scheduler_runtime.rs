use kernel_builder::config::{RuntimeConfig, RuntimePreference};
use kernel_builder::runtime::{
    container_command_args, detect_runtime, ContainerCommandSpec, HostNativeRuntime, RuntimeKind,
};
use std::{
    fs,
    process::Command,
    sync::{Arc, Mutex},
};

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

fn scheduler_request(source_root: String) -> BuildRequest {
    BuildRequest {
        source_root: source_root.into(),
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
    let source = init_source_repo(dir.path().join("linux").as_path());
    let store = Store::open(dir.path().join("kbs.db")).unwrap();
    let job = store
        .enqueue(scheduler_request(source.to_string_lossy().to_string()))
        .unwrap();
    let runtime = Arc::new(RecordingRuntime::default());
    let mut config =
        Config::for_test_with_allowlist(vec![source.to_string_lossy().to_string().into()]);
    config.storage.workspace_root = dir
        .path()
        .join("workspaces")
        .to_string_lossy()
        .to_string()
        .into();
    config.storage.artifact_root = dir
        .path()
        .join("artifacts")
        .to_string_lossy()
        .to_string()
        .into();
    config.storage.log_root = dir.path().join("logs").to_string_lossy().to_string().into();
    let scheduler = Scheduler::new(config, store, runtime);

    scheduler.run_one_queued_job().await.unwrap();

    assert_eq!(
        scheduler.get_job(job.id).unwrap().state,
        JobState::Succeeded
    );
}

#[test]
fn container_command_disables_network_by_default() {
    let build_command = ["make".into(), "ARCH=arm64".into(), "Image".into()];
    let args = container_command_args(ContainerCommandSpec {
        kind: RuntimeKind::Podman,
        image: "image:latest",
        source_root: "/src",
        output_root: "/out",
        build_command: &build_command,
        network_enabled: false,
        memory_limit: None,
        cpu_limit: None,
    })
    .unwrap();

    assert!(args.contains(&"--network=none".into()));
    assert!(!args.iter().any(|arg| arg.contains("docker.sock")));
    assert!(!args.iter().any(|arg| arg.contains("/logs/job.log")));
    assert!(args.contains(&"/src:/src:ro,Z".into()));
    assert!(args.contains(&"/out:/out:rw,Z".into()));
}

#[test]
fn container_command_applies_resource_limits() {
    let build_command = ["make".into(), "-j10".into(), "bzImage".into()];
    let args = container_command_args(ContainerCommandSpec {
        kind: RuntimeKind::Podman,
        image: "image:latest",
        source_root: "/src",
        output_root: "/out",
        build_command: &build_command,
        network_enabled: false,
        memory_limit: Some("16g"),
        cpu_limit: Some("10"),
    })
    .unwrap();

    assert!(args.contains(&"--memory=16g".into()));
    assert!(args.contains(&"--cpus=10".into()));
}

#[test]
fn docker_container_mounts_do_not_use_podman_relabel_options() {
    let build_command = ["make".into(), "bzImage".into()];
    let args = container_command_args(ContainerCommandSpec {
        kind: RuntimeKind::Docker,
        image: "image:latest",
        source_root: "/src",
        output_root: "/out",
        build_command: &build_command,
        network_enabled: false,
        memory_limit: None,
        cpu_limit: None,
    })
    .unwrap();

    assert!(args.contains(&"/src:/src:ro".into()));
    assert!(args.contains(&"/out:/out:rw".into()));
}

#[tokio::test]
async fn host_native_runtime_runs_command_and_writes_log() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("job.log");
    let runtime = HostNativeRuntime;
    let command = RuntimeCommand {
        program: "sh".into(),
        args: vec!["-c".into(), "printf hello".into()],
        source_root: dir.path().to_string_lossy().to_string().into(),
        output_root: dir.path().join("out").to_string_lossy().to_string().into(),
        log_path: log_path.to_string_lossy().to_string().into(),
    };

    let exit = runtime.run(JobId::new(), command).await.unwrap();

    assert_eq!(exit.code, Some(0));
    assert_eq!(std::fs::read_to_string(log_path).unwrap(), "hello");
}

fn init_source_repo(path: &std::path::Path) -> std::path::PathBuf {
    fs::create_dir(path).unwrap();
    run_git(path, &["init"]);
    run_git(path, &["config", "user.email", "builder@example.invalid"]);
    run_git(path, &["config", "user.name", "Kernel Builder"]);
    fs::write(path.join("Makefile"), "all:\n\t@true\n").unwrap();
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "-m", "init"]);
    path.to_path_buf()
}

fn run_git(repo: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
