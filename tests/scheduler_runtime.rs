use kernel_builder::config::{RuntimeConfig, RuntimePreference};
use kernel_builder::runtime::{detect_runtime, RuntimeKind};

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
