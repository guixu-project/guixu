<!--
Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
SPDX-License-Identifier: Apache-2.0
-->

# AGENTS.md

This file provides AI agent instructions for working with the Guixu codebase.

## Project Structure

Guixu is a Rust workspace with these key crates:

| Crate | Purpose |
|-------|---------|
| `data-core` | Types, config, identity |
| `data-storage` | DuckDB trace storage, RocksDB metadata stores |
| `data-agent` | Workflow orchestration, memory, planner |
| `data-search` | Dataset search, intent parsing, adapters |
| `data-valuation` | TCV scoring, free/paid evaluators |
| `data-trading` | Payment routing, wallet |
| `data-publish` | Dataset publishing |
| `data-auth` | Privacy, credentials, watermarks |
| `data-attestation` | Buyer reviews, seller reputation |
| `data-p2p` | libp2p networking, DHT, torrent |
| `data-mcp-server` | MCP server (HTTP + stdio) |
| `data-node` | CLI binary |

## Development Standards

**Before every commit, run:**
```bash
python3 scripts/check_english_comments.py && cargo fmt && cargo clippy -- -D warnings
```

### Key Rules
- SPDX headers required on all source files: `// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University // SPDX-License-Identifier: Apache-2.0`
- Comments must be in English
- Use `cargo fmt` (4 spaces, 100 char max width)
- Use `cargo clippy -- -D warnings` (no warnings allowed)
- New Rust files: run `python3 scripts/spdx_headers.py fix <file>`

### Adding Dependencies
- Add to `[workspace.dependencies]` in `Cargo.toml`
- Internal crates use `data-*` prefix
- Specify minimal features (avoid `features = ["full"]`)

### Adding New Crates
1. `cargo new --lib crates/data-new-crate`
2. Add to `Cargo.toml` workspace members
3. Add SPDX header with `python3 scripts/spdx_headers.py fix`

## Threading Constraints

**DuckDB `Connection` is NOT `Sync`!** It uses `RefCell` internally.

When adding tracing or async code that accesses shared state:
- DO NOT store `Arc<TraceStore>` in structs that need to be `Send`
- DO NOT hold lock guards across `.await` points
- Use `tokio::task::spawn_blocking` for blocking operations
- Use `try_write()` or `try_read()` instead of `write().await` when you need non-blocking access

## MCP Handler Template

```rust
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
```

---

<!-- BEGIN GUIXU MCP -->
Use the Guixu MCP server for dataset acquisition workflows.

A request does not need to explicitly mention datasets. If the task implies a need for external training data, labeled examples, benchmark data, or candidate dataset selection, treat it as a Guixu MCP workflow trigger.

This includes requests to:
- train, build, write, improve, or evaluate a model, classifier, detector, segmenter, retriever, or ranking system when the task depends on acquiring or selecting data
- parse a natural-language data request
- search for datasets
- evaluate dataset suitability or value
- purchase a dataset
- record post-use dataset feedback

Before writing model code or proposing a training implementation, first determine whether the task requires dataset acquisition or dataset evaluation through Guixu MCP. If it does, use Guixu MCP first.

When relevant, use these tools in order:
1. intent_parse
2. dataset_search
3. dataset_evaluate
4. dataset_purchase
5. dataset_feedback

Do not use Guixu MCP when the task is purely about local code changes, refactoring, formatting, UI polish, or debugging unrelated to data selection or data procurement, unless the user explicitly asks to use MCP.
<!-- END GUIXU MCP -->
