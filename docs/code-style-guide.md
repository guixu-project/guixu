<!--
Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
SPDX-License-Identifier: Apache-2.0
-->

# Guixu Code Style Guide

This project enforces a consistent code style across all contributors using Rust's standard tooling: `rustfmt` for formatting, `clippy` for linting, and CI for enforcement.

## Quick Start

Before committing, run:

```bash
cargo fmt
cargo clippy -- -D warnings
```

If both pass with no output/errors, your code is ready to commit.

## Tooling Overview

| Layer | Tool | Purpose |
|-------|------|---------|
| Formatting | `rustfmt` via `cargo fmt` | Consistent code layout |
| Linting | `clippy` via `cargo clippy` | Idiomatic Rust, bug prevention |
| CI | GitHub Actions | Blocks non-compliant PRs |
| Editor | rust-analyzer | Auto-format on save |

## 1. Formatting — `rustfmt`

Project rules are defined in [`rustfmt.toml`](../rustfmt.toml) at the workspace root.

Key settings:
- Max line width: **100** characters
- Indent: **4 spaces** (no tabs)

```bash
# Format all code
cargo fmt

# Check without modifying (used in CI)
cargo fmt -- --check
```

## 2. Linting — `clippy`

Lint levels are configured in [`Cargo.toml`](../Cargo.toml) under `[workspace.lints.clippy]`.

```bash
# Run clippy, treat warnings as errors
cargo clippy -- -D warnings
```

If clippy flags something you believe is a false positive, suppress it locally with an explanation:

```rust
#[allow(clippy::lint_name)] // reason: <explain why this is acceptable>
```

Do not add blanket `#[allow]` at the module or crate level.

## 3. CI Enforcement

The CI pipeline (`ci.yml`) runs `cargo fmt -- --check` and `cargo clippy -- -D warnings` on every push and PR to `main`. PRs that fail these checks cannot be merged.

## 4. Editor Setup (rust-analyzer)

Add to your VS Code `settings.json`:

```json
{
  "rust-analyzer.check.command": "clippy",
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

For other editors, enable format-on-save with `rustfmt` and set clippy as the check command.

## 5. Git Pre-Commit Hook

A pre-commit hook is included in `.githooks/`. It runs `cargo fmt --check` and `cargo clippy` before every commit. To enable it after cloning:

```bash
git config core.hooksPath .githooks
```

If the hook blocks your commit, run `cargo fmt` to fix formatting, then address any clippy warnings.

## Summary

1. Write code → rust-analyzer auto-formats on save
2. Before commit → `cargo fmt && cargo clippy -- -D warnings`
3. Push / PR → CI enforces the same checks
