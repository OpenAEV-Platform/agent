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

## CI Pipeline (CircleCI)

**6 platforms:** Linux, macOS, Windows × x86_64, ARM64

**PR/Dev Checks (`*_compile`):** `cargo check`, `cargo fmt --check` (Windows only), `cargo build --release`, `cargo test --release`

**Main/Release (`*_build`):** Same checks (except fmt) + installers (NSIS for Windows) + JFrog upload

**Common CI failures:** Formatting (Windows only), compilation errors, test failures

## Testing & Common Issues

```bash
cargo test                     # All tests ~35s
cargo test test_name           # Specific test
cargo test -- --nocapture      # Verbose
cargo install cargo-llvm-cov && cargo llvm-cov --html  # Coverage
```

**Issues:** Config not found (set `env=development`); Clippy warnings (`.to_string()` in format args, unnecessary `unwrap_err()`); Windows code needs Windows runners; Network test may fail with strict SSL

## Architecture & Configuration

**Threading:** 3 threads - keep-alive (`keep_alive.rs`), job listener (`agent_job.rs`), cleanup (`agent_cleanup.rs`)

**HTTP Client:** `reqwest` with optional TLS verification, proxy support, custom headers

**Job Execution:** `agent_exec.rs` manages runtime downloads, payload execution, working directories (`runtimes/`, `payloads/`)

**Config Modes:** Development (reads `config/default.toml`); Production (reads `openaev-agent-config` next to executable)

**Key Dependencies:** reqwest (rustls-tls), config, serde/serde_json, tracing (JSON), rolling-file, network-interface, mid (locked v3.0.2), windows-service

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

**When reviewing code, focus on:**

### Security Critical Issues
- Check for hardcoded secrets, API keys, or credentials
- Look for SQL injection and XSS vulnerabilities
- Verify proper input validation and sanitization
- Review authentication and authorization logic

### Performance Red Flags
- Identify N+1 database query problems
- Spot inefficient loops and algorithmic issues
- Check for memory leaks and resource cleanup
- Review caching opportunities for expensive operations

### Code Quality Essentials
- Functions should be focused and appropriately sized
- Use clear, descriptive naming conventions
- Ensure proper error handling throughout

### Review Style
- Be specific and actionable in feedback
- Explain the "why" behind recommendations
- Acknowledge good patterns when you see them
- Ask clarifying questions when code intent is unclear

**Always prioritize security vulnerabilities and performance issues that could impact users.**

**Always suggest changes to improve readability.** Example:
```rust
// Instead of inline validation (problematic - multiple unwraps):
if user.email.is_some() && user.email.as_ref().unwrap().contains('@') {
    submit_button.enabled = true;
}

// Consider extracting validation (safer and more readable):
fn is_valid_email(email: &Option<String>) -> bool {
    email.as_ref().map_or(false, |e| e.contains('@') && e.len() > 5)
}
submit_button.enabled = is_valid_email(&user.email);
```

**PR Checklist (from template):**
- I consider the submitted work as finished
- I tested the code for its functionality
- I wrote test cases for the relevant uses case
- I added/update the relevant documentation (either on github or on notion)
- Where necessary I refactored code to improve the overall quality
- For bug fix → I implemented a test that covers the bug

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
