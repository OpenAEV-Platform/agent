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

**Rust Version:** 1.92.0+ (uses 2021 edition)

### Prerequisites
- Install Rust via [rustup](https://rustup.rs/)
- Cargo is bundled with Rust

### Build Commands

```bash
# Check compilation without building
cargo check                    # ~25-30 seconds first run

# Build debug binary
cargo build                    # Output: target/debug/openaev-agent

# Build release binary
cargo build --release          # ~60 seconds; Output: target/release/openaev-agent
```

**Platform-Specific Builds (Linux):**
```bash
# For Linux musl static builds (used in CI):
rustup target add x86_64-unknown-linux-musl
cargo build --target=x86_64-unknown-linux-musl --release
strip ./target/x86_64-unknown-linux-musl/release/openaev-agent
```

### Code Quality (CRITICAL - CI will fail if these don't pass)

**Always run these before committing:**

```bash
# 1. Format check (REQUIRED)
cargo fmt -- --check           # Check formatting
cargo fmt                      # Auto-fix formatting issues

# 2. Linting (REQUIRED - zero warnings policy)
cargo clippy -- -D warnings    # Fails on ANY warning
cargo fix --clippy             # Auto-fix some clippy issues

# 3. Tests (REQUIRED)
cargo test                     # ~35 seconds
cargo test --release          # CI uses release mode for tests
```

**Known Issue:** As of this writing, there are 5 clippy warnings related to `.to_string()` in format args and unnecessary `unwrap_err()` calls. These MUST be fixed for CI to pass. One test (`test_unsecured_certificate_acceptance`) is known to fail intermittently.

### Running the Agent Locally

The agent requires a configuration file to run. For development:

```bash
# Set development mode (reads config/default.toml or config/development.toml)
env=development cargo run -- start

# Production mode (default) requires config file at:
# target/debug/openaev-agent-config (or next to the executable)
```

**Log Location:** `target/debug/openaev-agent.log` (JSON formatted)

**Config Structure:** See `config/default.toml` for required fields:
- `openaev.url` - Platform URL
- `openaev.token` - Access token
- `openaev.unsecured_certificate` - Allow self-signed certs
- `openaev.with_proxy` - Use system proxy
- `openaev.installation_mode` - "service-user" or "session-user"

### Security Audit

```bash
# Install cargo-audit if not present
cargo install cargo-audit

# Check for vulnerabilities
cargo audit

# Update dependencies
cargo update
```

**Note:** Cargo audit is run in CI and will block releases if vulnerabilities are found.

## Continuous Integration (CircleCI)

**Pipeline:** `.circleci/config.yml` - Builds for 6 platforms (Linux, macOS, Windows × x86_64, ARM64)

**PR/Development Branch Checks (`*_compile` jobs):**
1. `cargo check` - Compilation check
2. `cargo fmt -- --check` - Format validation
3. `cargo build --release` - Release build
4. `cargo test --release` - Test suite

**Main/Release Branch (`*_build` jobs):**
- Same checks as compile jobs
- Additional: Builds installers (NSIS for Windows)
- Uploads artifacts to JFrog Artifactory

**Failing CI?** Most common reasons:
- Clippy warnings (use `cargo clippy -- -D warnings` locally)
- Formatting issues (use `cargo fmt`)
- Test failures (run `cargo test` locally)

## Testing Strategy

```bash
# Run all tests
cargo test                     # ~35 seconds

# Run specific test
cargo test test_name

# Run with verbose output
cargo test -- --nocapture

# Code coverage (requires cargo-llvm-cov)
cargo install cargo-llvm-cov
cargo llvm-cov --html          # Output: target/llvm-cov/html/
```

**Test Files:** Located in `src/tests/` directory.

## Common Issues & Workarounds

### Issue 1: Config File Not Found
**Error:** `configuration file "/path/to/openaev-agent-config" not found`
**Fix:** Set `env=development` environment variable or create the config file.

### Issue 2: Clippy Warnings
**Known Warnings:**
- `.to_string()` in format args (remove `.to_string()`)
- Unnecessary `unwrap_err()` after `is_err()` check (use `if let Err(e)` pattern)

**Fix:** Address each warning individually. Use `cargo fix --clippy` for auto-fixes.

### Issue 3: Windows-Specific Code
**Context:** Windows service code is in `src/windows/service.rs`. It uses the `windows-service` crate.
**Testing:** Windows-specific code can only be tested on Windows runners.

### Issue 4: Network Tests
**Note:** The test `test_unsecured_certificate_acceptance` may fail in environments with strict SSL policies.

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

- **reqwest** - HTTP client (with rustls-tls)
- **config** - Configuration management
- **serde/serde_json** - Serialization
- **tracing** - Logging (JSON format)
- **rolling-file** - Log rotation
- **network-interface** - Network info
- **mid** - Machine ID (locked to v3.0.2)
- **windows-service** - Windows service support (Windows only)

**Locked Version:** `mid = "=3.0.2"` - Do not update without testing.

## Making Changes

1. **Always run quality checks before committing:**
   ```bash
   cargo fmt && cargo clippy -- -D warnings && cargo test
   ```

2. **For installer changes:** Update all three platforms (Linux, macOS, Windows). See `installer/README.md`.

3. **For API changes:** Check impact on `api/mod.rs`, `register_agent.rs`, `manage_jobs.rs`.

4. **For config changes:** Update `config/settings.rs` and `config/default.toml`.

5. **Cross-platform code:** Test on all supported platforms or use CI to validate.

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
