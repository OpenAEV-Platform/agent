# OpenAEV Agent - Copilot Coding Instructions

## Repository Overview

**OpenAEV Agent** is a cross-platform system agent written in Rust for the OpenAEV security platform. It runs on Linux (x86_64, ARM64), macOS (x86_64, ARM64), and Windows (x86_64, ARM64). The agent communicates with the OpenAEV platform via HTTP APIs, executes security tasks, and manages payloads/runtimes locally.

**Repository Stats:** ~1200 lines of Rust code across 21 source files. Total size: 1.5GB (includes target/ directory artifacts).

## Project Structure

```
/
├── src/                     # Source code (~21 .rs files)
│   ├── main.rs             # Entry point, logging setup, service detection
│   ├── api/                # HTTP client and API communication
│   │   ├── mod.rs          # Client module with headers, proxy, TLS config
│   │   ├── register_agent.rs
│   │   └── manage_jobs.rs
│   ├── config/             # Configuration management
│   │   ├── settings.rs     # Loads config from env vars or files
│   │   └── execution_details.rs
│   ├── process/            # Core agent logic
│   │   ├── keep_alive.rs   # Agent registration/heartbeat
│   │   ├── agent_job.rs    # Job listening/polling
│   │   ├── agent_exec.rs   # Job execution engine
│   │   └── agent_cleanup.rs
│   ├── windows/            # Windows service integration
│   ├── common/             # Shared error models
│   └── tests/              # Unit and integration tests
├── config/default.toml     # Default config (for development)
├── installer/              # Platform-specific installers
│   ├── linux/              # Shell scripts (.sh)
│   ├── macos/              # Shell scripts (.sh)
│   └── windows/            # PowerShell (.ps1) and NSIS (.nsi)
├── .circleci/config.yml    # Primary CI/CD pipeline
├── .github/workflows/      # GitHub Actions (release, labels)
├── Cargo.toml              # Rust package manifest
└── .cargo/config.toml      # Cargo aliases (lint, fmtcheck, fmtfix)
```

## Build & Development

**Rust Version:** 1.92.0+ (2021 edition)

**Prerequisites:** [Rust](https://rustup.rs/) (includes Cargo)

```bash
# Check compilation (~25-30s first run)
cargo check

# Build debug/release
cargo build                    # Output: target/debug/openaev-agent
cargo build --release          # ~60s; Output: target/release/openaev-agent

# Linux musl builds (CI)
rustup target add x86_64-unknown-linux-musl
cargo build --target=x86_64-unknown-linux-musl --release
strip ./target/x86_64-unknown-linux-musl/release/openaev-agent
```

### Code Quality

**Always run before committing:**

```bash
# 1. Format (REQUIRED in CI - Windows job)
cargo fmt -- --check           # Check
cargo fmt                      # Auto-fix

# 2. Lint (RECOMMENDED - not in CI)
cargo clippy -- -D warnings    # Fails on warnings
cargo fix --clippy             # Auto-fix

# 3. Tests (REQUIRED in CI)
cargo test                     # ~35s
cargo test --release          # CI uses release mode
```

**Known Issues:** 5 clippy warnings (`.to_string()` in format args, unnecessary `unwrap_err()`); 1 intermittent test failure (`test_unsecured_certificate_acceptance`).

### Running Locally

Development mode (reads `config/default.toml` or `config/development.toml`):
```bash
env=development cargo run -- start
```

Production requires config at `target/debug/openaev-agent-config` (or next to executable).

**Logs:** `target/debug/openaev-agent.log` (JSON format)

**Config fields:** `openaev.url`, `openaev.token`, `openaev.unsecured_certificate`, `openaev.with_proxy`, `openaev.installation_mode` (see `config/default.toml`)

### Security Audit

```bash
cargo install cargo-audit
cargo audit                    # Check vulnerabilities
cargo update                   # Update dependencies
```

**Note:** cargo-audit installed but not run in CI (macos_x86_64_compile installs it).

## Continuous Integration (CircleCI)

**Pipeline:** `.circleci/config.yml` - Builds for 6 platforms (Linux, macOS, Windows × x86_64, ARM64)

**PR/Development Branch Checks (`*_compile` jobs):**
1. `cargo check` - Compilation check
2. `cargo fmt -- --check` - Format validation (Windows job only)
3. `cargo build --release` - Release build
4. `cargo test --release` - Test suite

**Main/Release Branch (`*_build` jobs):**
- Same checks as compile jobs (except fmt)
- Additional: Builds installers (NSIS for Windows)
- Uploads artifacts to JFrog Artifactory

**Failing CI?** Most common reasons:
- Formatting issues (use `cargo fmt`) - only checked on Windows compile job
- Compilation errors (use `cargo check`)
- Test failures (run `cargo test --release` locally)

## Testing Strategy

```bash
cargo test                     # All tests ~35s
cargo test test_name           # Specific test
cargo test -- --nocapture      # Verbose output

# Coverage (requires cargo-llvm-cov)
cargo install cargo-llvm-cov
cargo llvm-cov --html          # Output: target/llvm-cov/html/
```

**Test Files:** `src/tests/` directory.

## Common Issues & Workarounds

**Config File Not Found:** Set `env=development` or create config file.

**Clippy Warnings:** `.to_string()` in format args (remove it); unnecessary `unwrap_err()` after `is_err()` (use `if let Err(e)`). Fix: `cargo fix --clippy`

**Windows-Specific Code:** `src/windows/service.rs` uses `windows-service` crate. Test only on Windows runners.

**Network Tests:** `test_unsecured_certificate_acceptance` may fail with strict SSL policies.

## Architecture Notes

**Threading Model:** Agent uses 3 threads:
1. **Keep-alive thread** (`keep_alive.rs`) - Registers agent, sends heartbeats
2. **Job listener thread** (`agent_job.rs`) - Polls for new jobs
3. **Cleanup thread** (`agent_cleanup.rs`) - Removes old payloads/runtimes

**HTTP Client:** Uses `reqwest` with:
- Optional TLS verification (`unsecured_certificate`)
- Optional system proxy support (`with_proxy`)
- Custom headers (token, machine ID, hostname)

**Job Execution:** Jobs are executed via `agent_exec.rs` which manages:
- Runtime downloads
- Payload execution
- Working directory management (`runtimes/`, `payloads/`)

**Configuration:** Two modes:
- **Development:** Reads from `config/default.toml` or `config/development.toml`
- **Production:** Reads from `openaev-agent-config` next to executable

## Key Dependencies

- **reqwest** - HTTP client (rustls-tls), **config** - Config management, **serde/serde_json** - Serialization
- **tracing** - Logging (JSON), **rolling-file** - Log rotation, **network-interface** - Network info
- **mid** - Machine ID (locked to v3.0.2 - do not update without testing)
- **windows-service** - Windows service support

## Making Changes

1. **Always run quality checks before committing:**
   ```bash
   cargo fmt && cargo clippy -- -D warnings && cargo test
   ```

2. **For installer changes:** Update all three platforms (Linux, macOS, Windows). See `installer/README.md`.

3. **For API changes:** Check impact on `api/mod.rs`, `register_agent.rs`, `manage_jobs.rs`.

4. **For config changes:** Update `config/settings.rs` and `config/default.toml`.

5. **Cross-platform code:** Test on all supported platforms or use CI to validate.

## Code Review Guidelines

**Before submitting PR:**
- Run `cargo fmt && cargo clippy -- -D warnings && cargo test`
- Test functionality manually if code changes affect runtime behavior
- Add/update tests for new features or bug fixes
- Update documentation if changing APIs or behavior
- Verify CI passes on all 6 platforms

**PR Checklist (from template):**
- Code is finished and ready for review
- Functionality tested
- Test cases written for relevant use cases
- Documentation added/updated
- Code refactored for quality where necessary
- Bug fixes include tests covering the bug

## Release Process

Releases are managed via `scripts/release.py` (Python 3.8+):
```bash
pip install -r scripts/requirements.txt
python scripts/release.py <branch> <old_version> <new_version> <github_token>
```

**What it does:**
- Updates version in `Cargo.toml`
- Creates git tag
- Triggers CI builds
- Generates GitHub release notes

## Trust These Instructions

These instructions have been validated by running all commands and reviewing all CI pipelines. Only search for additional information if:
- You encounter an error not documented here
- You need to understand internal implementation details
- These instructions are incomplete or incorrect for your specific task

**Last Updated:** 2026-01-09
