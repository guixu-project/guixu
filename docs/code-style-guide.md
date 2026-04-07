<!--
Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
SPDX-License-Identifier: Apache-2.0
-->

# Guixu Code Style Guide

This project enforces a consistent code style across all contributors using Rust's standard tooling, repository scripts, and CI for enforcement.

## Quick Start

Before committing, run:

```bash
cargo fmt
cargo clippy -- -D warnings
python3 scripts/check_english_comments.py
```

If both pass with no output/errors, your code is ready to commit.

## Tooling Overview

| Layer | Tool | Purpose |
|-------|------|---------|
| Formatting | `rustfmt` via `cargo fmt` | Consistent code layout |
| Linting | `clippy` via `cargo clippy` | Idiomatic Rust, bug prevention |
| Comment language | `scripts/check_english_comments.py` | Enforce English-only code comments |
| Commit messages | `scripts/check_english_commit_message.py` | Enforce English-only commit messages |
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

The CI pipeline (`ci.yml`) runs the SPDX header check, English comment check, `cargo fmt -- --check`, and `cargo clippy -- -D warnings` on every push and PR to `main`. PRs that fail these checks cannot be merged.

## 4. Code Comment Language

Code comments must be written in English.

The repository enforces this with:

```bash
python3 scripts/check_english_comments.py
```

Current scope:
- Rust line comments and block comments
- Python, shell, YAML, and TOML `#` comments

The checker rejects CJK characters in comments so non-English comments are caught before commit and in CI.

## 5. Commit Message Language

Commit messages must be written in English.

The repository enforces this locally with the `commit-msg` hook:

```bash
python3 scripts/check_english_commit_message.py .git/COMMIT_EDITMSG
```

The checker rejects CJK characters in commit messages, which prevents non-English commit messages from being recorded through the standard git hook flow.

## 6. Editor Setup (rust-analyzer)

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

## 7. Git Hooks

Git hooks are included in `.githooks/`. They run the following checks:

- `pre-commit`: SPDX fixer, English comment check, `cargo fmt`, and `cargo clippy`
- `commit-msg`: English commit message check

To enable them after cloning:

```bash
git config core.hooksPath .githooks
```

If the hook blocks your commit, run `cargo fmt` to fix formatting, then address any clippy warnings.

## Summary

1. Write code → rust-analyzer auto-formats on save
2. Before commit → `python3 scripts/check_english_comments.py && cargo fmt && cargo clippy -- -D warnings`
3. Push / PR → CI enforces the same checks
