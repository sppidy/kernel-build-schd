use kernel_builder::config::{Config, RuntimePreference};
use kernel_builder::model::{BuildRequest, EnvVar};
use kernel_builder::policy::Policy;
use serde_json::json;

#[test]
fn default_config_rejects_empty_source_allowlist() {
    let config = Config::for_test_with_allowlist(vec![]);
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("source allowlist"));
}

#[test]
fn valid_test_config_keeps_host_native_disabled() {
    let config = Config::for_test_with_allowlist(vec!["/src".into()]);

    config.validate().unwrap();

    assert_eq!(config.runtime.preference, RuntimePreference::Auto);
    assert!(!config.security.enable_host_native);
}

fn request_for(source_root: &str) -> BuildRequest {
    BuildRequest {
        source_root: Some(source_root.into()),
        source_url: None,
        tree_name: None,
        git_ref: None,
        profile: None,
        arch: "x86_64".into(),
        config_target: "defconfig".into(),
        config_fragments: vec![],
        make_targets: vec!["bzImage".into()],
        env: vec![],
        timeout_secs: None,
        priority: 0,
        artifact_patterns: vec!["arch/x86/boot/bzImage".into()],
    }
}

fn request_for_url(source_url: &str) -> BuildRequest {
    serde_json::from_value(json!({
        "source_url": source_url,
        "arch": "x86_64",
        "config_target": "defconfig",
        "config_fragments": [],
        "make_targets": ["bzImage"],
        "env": [],
        "priority": 0,
        "artifact_patterns": ["arch/x86/boot/bzImage"]
    }))
    .unwrap()
}

#[test]
fn policy_rejects_source_outside_allowlist() {
    let config = Config::for_test_with_allowlist(vec!["/allowed".into()]);
    let policy = Policy::new(config.security);

    let err = policy
        .validate_request(&request_for("/tmp/linux"))
        .unwrap_err();

    assert!(err.to_string().contains("outside allowlist"));
}

#[test]
fn policy_accepts_allowed_clone_url_without_source_root() {
    let mut config = Config::for_test_with_allowlist(vec!["/allowed".into()]);
    config.security.clone_url_allowlist = vec!["https://github.com/".into()];
    let policy = Policy::new(config.security);

    policy
        .validate_request(&request_for_url("https://github.com/example/linux.git"))
        .unwrap();
}

#[test]
fn policy_rejects_unallowlisted_clone_url() {
    let mut config = Config::for_test_with_allowlist(vec!["/allowed".into()]);
    config.security.clone_url_allowlist = vec!["https://github.com/spidy/".into()];
    let policy = Policy::new(config.security);

    let err = policy
        .validate_request(&request_for_url("https://github.com/other/linux.git"))
        .unwrap_err();

    assert!(err.to_string().contains("clone URL"));
}

#[test]
fn policy_rejects_denied_environment_variable() {
    let config = Config::for_test_with_allowlist(vec!["/allowed".into()]);
    let policy = Policy::new(config.security);
    let mut request = request_for("/allowed/linux");
    request.env = vec![EnvVar {
        key: "LD_PRELOAD".into(),
        value: "/tmp/libhack.so".into(),
    }];

    let err = policy.validate_request(&request).unwrap_err();

    assert!(err.to_string().contains("denied environment"));
}
