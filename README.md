# OpenAEV Agent Test

[![Website](https://img.shields.io/badge/website-openaev.io-blue.svg)](https://openaev.io)
[![CircleCI](https://circleci.com/gh/OpenAEV-Platform/agent.svg?style=shield)](https://circleci.com/gh/OpenAEV-Platform/agent/tree/main)
[![GitHub release](https://img.shields.io/github/release/OpenAEV-Platform/agent.svg)](https://github.com/OpenAEV-Platform/agent/releases/latest)
[![Slack Status](https://img.shields.io/badge/slack-3K%2B%20members-4A154B)](https://community.filigran.io)

The following repository is used to store the OpenAEV agent for the platform. For performance and low level access, the agent is written in Rust. Please start your journey with https://doc.rust-lang.org/book.

---

## 🚀 Installation

Agent installation is fully managed by the OpenAEV platform.

You can find more information on the [official documentation](https://docs.openaev.io/latest/usage/openaev-agent/?h=agent).

## 🛠 Development

The agent is written in [Rust](https://www.rust-lang.org/). If you're new to Rust, start with [The Rust Book](https://doc.rust-lang.org/book).

### Prerequisites

- [Rust](https://rustup.rs/)
- [Cargo](https://doc.rust-lang.org/cargo/)
- Linux, macOS, or Windows

### Build

```bash
cargo build
```

---

## ✅ Running Tests

Run all tests (unit + integration):

```bash
cargo test
```

Run a specific test:

```bash
cargo test test_name
```

---

## 📊 Code Coverage

Requires [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov):

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --html
```

---

## 🧹 Code Quality Guidelines

### Clippy

Run locally:

```bash
cargo clippy -- -D warnings
```

Auto-fix:

```bash
cargo fix --clippy
```

Clippy runs in CI — all warnings must be fixed for the pipeline to pass.

---

### Rustfmt

Check formatting:

```bash
cargo fmt -- --check
```

Fix formatting:

```bash
cargo fmt
```

Rustfmt runs in CI to enforce formatting.

---

### Cargo Audit

Check dependencies for known vulnerabilities:

```bash
cargo audit
```

Update vulnerable packages:

```bash
cargo update
```

Audit is included in CI to block new vulnerabilities.

---

## 🧪 Tests in CI

All tests are run automatically in the CI pipeline using:

```bash
cargo test
```

Builds will fail if any tests or quality checks fail.

---

## 🛠 Troubleshooting in Development Mode

When running the agent in development mode using:

```bash
cargo run -- start
```

All logs are written to:

```
target/debug/openaev-agent.log
```

Check this file if something isn’t working or you need to debug an issue locally.

---

## 🧬 About

OpenAEV is developed by [Filigran](https://filigran.io), a company dedicated to building open-source security tooling.

<a href="https://filigran.io" alt="Filigran"><img src="https://github.com/OpenCTI-Platform/opencti/raw/master/.github/img/logo_filigran.png" width="300" /></a>
