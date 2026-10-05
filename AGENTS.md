# AGENTS.md

OpenAEV agent: a Rust binary installed on endpoints. It registers with the OpenAEV platform, polls for jobs and runs them. Targets Linux, macOS and Windows, on x86_64 and arm64.

## Layout

- `src/main.rs`: logger setup, Windows service detection, starts the worker threads.
- `src/process/`: keep-alive (registration and heartbeat), job polling, job execution, cleanup of old executions.
- `src/api/`: HTTP client (blocking `reqwest`, rustls, proxy and certificate options) and platform calls.
- `src/config/`: settings loading and execution context (user, elevation, service).
- `src/windows/`: Windows service integration.
- `src/tests/`: tests, mirroring the `src/` layout.
- `installer/`: install and upgrade scripts per OS and per install mode (service, service-user, session-user).

## Commands

```bash
cargo fmtcheck          # cargo fmt -- --check
cargo lint              # cargo clippy -- -D warnings
cargo test --locked
cargo audit
env=development cargo run
```

With `env=development`, settings come from `config/default.toml`, then `config/development.toml` if present. Without `env`, the agent reads `openaev-agent-config` next to the executable. Logs go to `openaev-agent.log` next to the executable.

## CI

`.github/workflows/agent-ci.yml`: fmt and audit, clippy per target, PSScriptAnalyzer on Windows scripts, tests and release builds per OS and arch, coverage, NSIS installers.

## Rules

- Code must build on every target: put OS-specific code behind `cfg` attributes.
- When changing an installer script, check the same script for the other OSes and install modes.
- Windows builds link the C runtime statically through `.cargo/config.toml`: do not set `RUSTFLAGS` in CI.
- Commit, PR and issue titles follow Conventional Commits with an issue reference, see `CONTRIBUTING.md`.
