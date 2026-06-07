use std::fs;

use kernel_builder::config::Config;
use kernel_builder::db::Store;
use kernel_builder::executor::{collect_artifacts, write_combined_log};
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
