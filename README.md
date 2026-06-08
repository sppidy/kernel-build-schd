# Kernel Builder

Single-machine Linux kernel build scheduler with a local daemon, SQLite queue,
container workers, CLI administration, and an MCP interface for agents.

## Commands

```sh
kbs --config config/example.toml config check
kbs --config config/example.toml daemon --foreground
kbs --config config/example.toml mcp
kbs --config config/example.toml status
kbs --config config/example.toml jobs list
```

Without `--config`, `kbs` checks `/etc/kernel-build-scheduler/config.toml` and
then `$HOME/.config/kernel-build-scheduler/config.toml`.

## Source Trees

Builds can use a local allowlisted source path, a clone URL allowed by
`security.clone_url_allowlist`, or a registered tree name.

One-off MCP scheduling can pass `source_url` directly:

```json
{
  "source_url": "https://github.com/example/linux.git",
  "git_ref": "custom/branch",
  "arch": "arm64",
  "config_target": "defconfig",
  "config_fragments": [],
  "make_targets": ["Image"],
  "env": [],
  "artifact_patterns": ["arch/arm64/boot/Image"]
}
```

Agents can also register a tree once with `register_source_tree`, then schedule
with `tree_name` and an optional `git_ref` override.

## Development

```sh
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
```

Do not enable host-native builds unless the build host is trusted for that job.
