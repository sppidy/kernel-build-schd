use std::fs;
use std::process::Command;

use kernel_builder::config::Config;
use kernel_builder::db::Store;
use kernel_builder::executor::{build_runtime_command, collect_artifacts, write_combined_log};
use kernel_builder::model::BuildRequest;

fn request_with_artifact(path: &str) -> BuildRequest {
    BuildRequest {
        source_root: "/allowed/linux".into(),
        git_ref: None,
        profile: None,
        arch: "x86_64".into(),
        config_target: "defconfig".into(),
        config_fragments: vec![],
        make_targets: vec!["modules_prepare".into()],
        env: vec![],
        timeout_secs: None,
        priority: 0,
        artifact_patterns: vec![path.into()],
    }
}

#[test]
fn artifact_collection_records_hash_and_size() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::for_test_with_allowlist(vec!["/allowed".into()]);
    config.storage.artifact_root = dir
        .path()
        .join("artifacts")
        .to_string_lossy()
        .to_string()
        .into();
    let store = Store::open(dir.path().join("kbs.db")).unwrap();
    let job = store.enqueue(request_with_artifact("vmlinux")).unwrap();
    let out_dir = dir.path().join("out");
    fs::create_dir_all(&out_dir).unwrap();
    fs::write(out_dir.join("vmlinux"), b"kernel").unwrap();

    let artifacts = collect_artifacts(&config, &store, job.id, &out_dir).unwrap();

    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].bytes, 6);
    assert_eq!(artifacts[0].sha256.len(), 64);
}

#[test]
fn combined_log_is_bounded_when_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("job.log");

    write_combined_log(&log_path, b"hello\nworld\n").unwrap();

    assert_eq!(fs::read_to_string(log_path).unwrap(), "hello\nworld\n");
}

#[test]
fn build_command_prepares_requested_git_ref_in_job_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("linux");
    fs::create_dir(&source).unwrap();
    run_git(&source, &["init"]);
    run_git(
        &source,
        &["config", "user.email", "builder@example.invalid"],
    );
    run_git(&source, &["config", "user.name", "Kernel Builder"]);
    fs::write(source.join("branch.txt"), "main").unwrap();
    fs::create_dir_all(source.join("fragments")).unwrap();
    fs::write(
        source.join("fragments/base.config"),
        "CONFIG_LOCALVERSION=\"-kbs\"\n",
    )
    .unwrap();
    run_git(&source, &["add", "."]);
    run_git(&source, &["commit", "-m", "main"]);
    run_git(&source, &["checkout", "-b", "topic/build-me"]);
    fs::write(source.join("branch.txt"), "topic").unwrap();
    run_git(&source, &["commit", "-am", "topic"]);

    let mut config =
        Config::for_test_with_allowlist(vec![source.to_string_lossy().into_owned().into()]);
    config.storage.workspace_root = dir
        .path()
        .join("workspaces")
        .to_string_lossy()
        .to_string()
        .into();
    config.storage.log_root = dir.path().join("logs").to_string_lossy().to_string().into();
    let store = Store::open(dir.path().join("kbs.db")).unwrap();
    let job = store
        .enqueue(BuildRequest {
            source_root: source.to_string_lossy().to_string().into(),
            git_ref: Some("topic/build-me".into()),
            profile: None,
            arch: "arm64".into(),
            config_target: "defconfig".into(),
            config_fragments: vec!["fragments/base.config".into()],
            make_targets: vec!["-j10".into(), "Image".into()],
            env: vec![],
            timeout_secs: None,
            priority: 0,
            artifact_patterns: vec!["arch/arm64/boot/Image".into()],
        })
        .unwrap();

    let command = build_runtime_command(&config, &job).unwrap();

    assert_eq!(
        fs::read_to_string(command.source_root.join("branch.txt")).unwrap(),
        "topic"
    );
    assert!(command.output_root.as_std_path().is_dir());
    let script = command.args.join("\n");
    assert!(script.contains("defconfig"));
    assert!(script.contains("merge_config.sh"));
    assert!(script.contains("-j10"));
    assert!(script.contains("Image"));
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
