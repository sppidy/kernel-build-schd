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

## Development

```sh
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
```

Do not enable host-native builds unless the build host is trusted for that job.
