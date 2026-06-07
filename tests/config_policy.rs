use kernel_builder::config::{Config, RuntimePreference};

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
