<!--
Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
SPDX-License-Identifier: Apache-2.0
-->

# Guixu Developer Guide

This guide helps AI agents and human developers understand Guixu's development environment, code conventions, and workflows.

## 1. Project Overview

Guixu is a self-evolving AI agent for autonomous data discovery and assetization. The codebase is organized as a Rust workspace with multiple crates:

```
guixu/
├── crates/
│   ├── core/          # Shared types, config, identity
│   ├── storage/        # DuckDB trace storage, RocksDB metadata
│   ├── agent/          # Workflow orchestration, memory, planner
│   ├── search/         # Dataset search, intent parsing, adapters
│   ├── valuation/      # TCV scoring, free/paid evaluators
│   ├── trading/        # Payment routing, wallet integration
│   ├── publish/        # Dataset publishing, encryption
│   ├── auth/           # Privacy, credentials, watermarks
│   ├── attestation/    # Buyer reviews, seller reputation
│   ├── p2p/            # libp2p networking, DHT, torrent
│   ├── node/           # CLI entry point, MCP install
│   └── plugins/mcp/    # MCP server (HTTP + stdio)
├── skills/guixu/       # OpenClaw skill definition
├── docs/               # Developer docs, spec docs
└── scripts/            # CI hooks, SPDX header tools
```

### Crate Naming Convention

All internal crates use the `data-` prefix:

| Crate | Path | Purpose |
|-------|------|---------|
| `data-core` | `crates/core` | Types, config, identity |
| `data-storage` | `crates/storage` | Trace, metadata, job stores |
| `data-agent` | `crates/agent` | Workflow, memory, planner |
| `data-search` | `crates/search` | Search engine, intent |
| `data-valuation` | `crates/valuation` | TCV evaluation |
| `data-trading` | `crates/trading` | Payment routing |
| `data-publish` | `crates/publish` | Publishing pipeline |
| `data-auth` | `crates/auth` | Privacy, credentials |
| `data-attestation` | `crates/attestation` | Reviews, reputation |
| `data-p2p` | `crates/p2p` | Networking |
| `data-node` | `crates/node` | CLI binary |
| `data-mcp-server` | `crates/plugins/mcp` | MCP plugin server |

## 2. Environment Setup

### For AI Agents

When setting up the development environment, follow this sequence:

```bash
# 1. Clone the repository
git clone https://github.com/guixu-project/guixu.git
cd guixu

# 2. Bootstrap git hooks (REQUIRED before any commit)
./bootstrap.sh

# 3. Build the project (first build is slow, subsequent incremental builds are fast)
cargo build

# 4. Verify the build succeeded
cargo build 2>&1 | tail -5
# Expected: "Finished `dev` profile" or "Finished `release` profile"
```

### Prerequisites

- **Rust 1.94+** (edition 2021)
- **Git** with configured hooks
- **Python 3.8+** (for scripts)
- **DuckDB** (bundled via `duckdb` crate, no separate install needed)
- **libclang** (may be required for rocksdb or polars on some platforms)

### Build Commands

```bash
# Debug build (fast, for development)
cargo build

# Release build (optimized, for deployment)
cargo build --release

# Build specific crate (faster for testing changes)
cargo build -p data-storage
cargo build -p data-mcp-server

# Check formatting and clippy without building
cargo fmt -- --check
cargo clippy -- -D warnings

# Run tests
cargo test

# Run tests for specific crate
cargo test -p data-storage
```

## 3. Code Style and Standards

**See [code-style-guide.md](code-style-guide.md) for the complete reference.**

### Quick Checklist Before Committing

```bash
# Run ALL checks in order
python3 scripts/check_english_comments.py && cargo fmt && cargo clippy -- -D warnings
```

### Key Rules

| Rule | Command | Notes |
|------|---------|-------|
| Formatting | `cargo fmt` | 4 spaces, 100 char max width |
| Linting | `cargo clippy -- -D warnings` | Treats warnings as errors |
| English comments | `python3 scripts/check_english_comments.py` | No CJK characters |
| SPDX headers | `python3 scripts/spdx_headers.py fix` | Add headers to new files |

### SPDX Header Format

Every source file must start with:

```rust
// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0
```

For new files, run: `python3 scripts/spdx_headers.py fix <file>`

## 4. Git Workflow

### Branch Naming

```
feature/<feature-name>
bugfix/<bug-description>
hotfix/<urgent-fix>
```

### Commit Message Format

```
<type>: <short description>

<longer description if needed>

<issue number if applicable>
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

### Pre-Commit Hooks

The `bootstrap.sh` script configures git hooks at `.githooks/`. These run automatically before each commit:

1. SPDX header check
2. English comment check
3. `cargo fmt`
4. `cargo clippy -- -D warnings`

If the pre-commit hook fails, fix the issues and try again:
```bash
cargo fmt
# Address clippy warnings
git commit
```

## 5. Adding New Crates

### Step 1: Create the crate

```bash
cargo new --lib crates/data-new-crate
```

### Step 2: Add to workspace

Edit `Cargo.toml`:

```toml
[workspace]
members = [
    # ... existing crates ...
    "crates/data-new-crate",
]

[workspace.dependencies]
data-new-crate = { path = "crates/data-new-crate" }
```

### Step 3: Add SPDX header

```bash
python3 scripts/spdx_headers.py fix crates/data-new-crate/src/lib.rs
```

### Step 4: Add exports

In your crate's `src/lib.rs`:

```rust
// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0
```

## 6. Adding New MCP Handlers

MCP handlers live in `crates/plugins/mcp/src/handlers/`.

### Template

```rust
// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde_json::Value;

use super::trace_hooks::with_trace;
use crate::state::AppState;

pub async fn handle(args: Value, state: &AppState) -> Result<String> {
    with_trace(&state.trace_manager, "mcp.handler_name", None, async {
        inner_handle(args, state).await
    })
    .await
}

async fn inner_handle(args: Value, state: &AppState) -> Result<String> {
    // Implementation
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "status": "success"
    }))?)
}
```

### Register in Catalog

Edit `crates/plugins/mcp/src/catalog.rs` to add your handler to the tool registry.

## 7. Adding New Trace Spans

When instrumenting code with tracing, follow this pattern:

```rust
// For WorkflowService - use the built-in trace manager
use data_storage::trace_manager::{SpanBuilder, SpanType};

pub async fn run(&self, task: DelegatedDataTask) -> anyhow::Result<JobResult> {
    let (trace_id, span_id) = if let Some(tm) = &self.trace_manager {
        let tm = tm.read().await;
        tm.start_trace("workflow.run", SpanType::Agent)
            .await
            .unwrap_or_else(|| (String::new(), String::new()))
    } else {
        (String::new(), String::new())
    };

    // ... your code ...

    // End span
    if let (Some(tm), true) = (&self.trace_manager, !trace_id.is_empty()) {
        let mut tm = tm.write().await;
        let builder = SpanBuilder::new(&trace_id, &span_id, None, "workflow.run", SpanType::Agent)
            .with_attribute("job_id", serde_json::json!(job_id));
        let builder = if result.is_err() {
            builder.with_error(result.as_ref().err().unwrap().to_string())
        } else {
            builder
        };
        tm.end_span(builder).await;
    }
}
```

## 8. Common Development Tasks

### Adding a Dependency

1. Add to `[workspace.dependencies]` in `Cargo.toml`
2. Use `data-*` prefix for internal crates
3. Specify features explicitly (avoid `features = ["full"]` for large crates)

```toml
# Good
tokio = { version = "1", features = ["rt-multi-thread", "sync"] }

# Avoid (too large)
tokio = { version = "1", features = ["full"] }
```

### Running Specific Tests

```bash
# Test a specific crate
cargo test -p data-storage

# Test with output
cargo test -p data-storage -- --nocapture

# Run a specific test
cargo test test_name_here -p data-storage
```

### Debug Logging

```bash
# All modules
RUST_LOG=debug cargo run --bin data-node start

# Specific module
RUST_LOG=debug,data_storage=trace cargo run --bin data-node start

# Filtered
RUST_LOG=info,cargo::ops=warn cargo run --bin data-node start
```

## 9. Troubleshooting

### "RefCell cannot be shared between threads"

This error occurs when a type containing `RefCell` (like DuckDB `Connection`) is stored in a shared `Arc` that needs to be `Send`. Solutions:

1. **Remove the type from the shared struct** - Pass it through channels or spawn_blocking instead
2. **Use `Arc<Mutex<T>>` instead of `Arc<RwLock<T>>`** - Only if you truly need mutual exclusion
3. **Spawn blocking tasks** - Use `tokio::task::spawn_blocking` for operations that need the type

### "Future is not Send"

This occurs when a future holds a `!Send` type across an `.await` point. Check:

1. Are you holding a lock guard across an await? (use `select` or drop immediately)
2. Does your `AppState` contain `!Send` types? (it shouldn't after the trace fix)

### Build Errors with `guixu-mcp-server`

```bash
# Full clean rebuild
cargo clean && cargo build -p data-mcp-server
```

## 10. Key Files Reference

| File | Purpose |
|------|---------|
| `Cargo.toml` | Workspace definition, dependencies |
| `rustfmt.toml` | Code formatting rules |
| `AGENTS.md` | AI agent instructions for dataset workflows |
| `docs/code-style-guide.md` | Detailed style reference |
| `docs/developer.md` | This file |
| `CONTRIBUTING.md` | Legal and practical contribution guide |
| `bootstrap.sh` | Git hooks setup script |
| `scripts/spdx_headers.py` | SPDX header checker/fixer |
| `scripts/check_english_comments.py` | Comment language checker |

## 11. Getting Help

- **Open an issue** for bugs, feature requests, or API changes
- **Discussion** for design questions or exploratory work
- **CLA.md** for licensing and copyright questions
