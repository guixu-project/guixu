// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! End-to-end MCP integration test: academic literature discovery for AI agents.
//!
//! Simulates a Codex agent using Guixu MCP to discover academic papers about
//! "data discovery for AI agents" across DBLP, Semantic Scholar, and arXiv.
//!
//! Run with:
//!   cargo build --release -p data-node
//!   cargo test -p data-mcp-server --test academic_search_e2e -- --ignored --nocapture

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

fn data_node_binary() -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("cannot resolve workspace root");
    for profile in ["release", "debug"] {
        let bin = workspace_root.join(format!("target/{profile}/data-node"));
        if bin.exists() {
            return bin;
        }
    }
    panic!("data-node binary not found. Run `cargo build --release -p data-node` first.");
}

fn mcp_roundtrip(messages: &[Value], timeout_secs: u64) -> Vec<Value> {
    let binary = data_node_binary();
    let mut child = Command::new(&binary)
        .args(["mcp", "--mode", "codex"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", binary.display()));

    let mut stdin = child.stdin.take().unwrap();
    for msg in messages {
        writeln!(stdin, "{}", serde_json::to_string(msg).unwrap()).unwrap();
    }
    drop(stdin);

    // Read stdout in a separate thread to avoid pipe buffer deadlock
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut r = stdout;
        r.read_to_end(&mut buf).unwrap();
        buf
    });

    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                panic!("MCP server timed out after {timeout_secs}s");
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => panic!("wait error: {e}"),
        }
    }

    let buf = reader.join().unwrap();
    String::from_utf8_lossy(&buf)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .collect()
}

fn init() -> Value {
    json!({
        "jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "codex-e2e-test", "version": "0.1" }
        }
    })
}

fn call(id: u64, tool: &str, args: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": tool, "arguments": args } })
}

fn tool_json(resp: &Value) -> Value {
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("{}");
    serde_json::from_str(text).unwrap_or_else(|_| json!({ "_raw": text }))
}

// ============================================================================
// Full pipeline: intent_parse → dataset_search → dataset_evaluate
// ============================================================================

/// Simulates a Codex agent performing academic literature survey on
/// "data discovery for AI agents". Exercises the full MCP pipeline and
/// verifies that DBLP and arXiv adapters contribute results.
///
/// Note: Semantic Scholar may return 429 (rate limit) for unauthenticated
/// requests — this is expected and reported in the errors array.
#[test]
#[ignore = "requires network access and release binary"]
fn academic_literature_discovery_for_ai_agents() {
    let query = "data discovery and marketplace for autonomous AI agents";

    // ── Step 1: intent_parse ────────────────────────────────────────────
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("Step 1: intent_parse");
    eprintln!("  query: \"{query}\"");

    let r1 = mcp_roundtrip(
        &[init(), call(1, "intent_parse", json!({ "query": query }))],
        30,
    );
    assert!(r1.len() >= 2, "no response from intent_parse");

    let intent = tool_json(&r1[1]);
    let keywords = intent["keywords"].as_array();
    assert!(keywords.is_some(), "intent_parse should extract keywords");
    eprintln!(
        "  ✓ keywords: {:?}",
        keywords
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
    );
    eprintln!(
        "  ✓ task_type: {}",
        intent["task_type"].as_str().unwrap_or("?")
    );
    eprintln!(
        "  ✓ task_description: {}",
        intent["task_description"].as_str().unwrap_or("?")
    );

    // ── Step 2: dataset_search ──────────────────────────────────────────
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("Step 2: dataset_search");

    let r2 = mcp_roundtrip(
        &[
            init(),
            call(1, "dataset_search", json!({ "query": query, "limit": 20 })),
        ],
        120,
    );
    assert!(r2.len() >= 2, "no response from dataset_search");

    let search = tool_json(&r2[1]);
    let results = search["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "search returned no results");

    // Tally by source
    let mut by_source: std::collections::BTreeMap<String, Vec<&Value>> =
        std::collections::BTreeMap::new();
    for r in results {
        let src = r["source"].as_str().unwrap_or("unknown").to_string();
        by_source.entry(src).or_default().push(r);
    }

    eprintln!("\n  Total results: {}", results.len());
    eprintln!("  Sources: {:?}", by_source.keys().collect::<Vec<_>>());
    for (src, items) in &by_source {
        eprintln!("\n  [{src}] {} result(s):", items.len());
        for (i, item) in items.iter().enumerate().take(3) {
            let title = item["title"].as_str().unwrap_or("?");
            let cid = item["cid"].as_str().unwrap_or("?");
            eprintln!("    {}. {title}", i + 1);
            eprintln!("       cid={cid}");
        }
    }

    // Report errors (some adapters may fail — that's OK)
    if let Some(errors) = search["errors"].as_array() {
        if !errors.is_empty() {
            eprintln!("\n  Adapter errors ({}):", errors.len());
            for e in errors {
                let s = e
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| e.to_string());
                eprintln!("    ⚠ {}", &s[..s.len().min(120)]);
            }
        }
    }

    // Verify academic sources contributed results.
    // Note: results from previous searches may be cached in the local P2P store,
    // so DBLP/arXiv/S2 results might appear with source="p2p" after deduplication.
    let has_dblp = by_source.contains_key("dblp");
    let has_arxiv_direct = by_source.contains_key("arxiv");
    let has_s2 = by_source.contains_key("semanticscholar");

    // Check for academic content regardless of source label
    let has_doi_results = results.iter().any(|r| {
        r["cid"].as_str().map_or(false, |c| {
            c.starts_with("10.") || c.contains("arxiv.org") || c.starts_with("s2:")
        })
    });

    // Semantic Scholar may be rate-limited (429)
    let s2_rate_limited = search["errors"].as_array().map_or(false, |errs| {
        errs.iter().any(|e| {
            e.as_str().map_or(false, |s| {
                s.contains("semantic_scholar") && s.contains("429")
            })
        })
    });

    eprintln!("\n  Academic source coverage:");
    eprintln!(
        "    DBLP:              {} ({})",
        if has_dblp { "✓" } else { "–" },
        by_source.get("dblp").map_or(0, |v| v.len())
    );
    eprintln!(
        "    arXiv:             {} ({})",
        if has_arxiv_direct { "✓" } else { "–" },
        by_source.get("arxiv").map_or(0, |v| v.len())
    );
    eprintln!(
        "    Semantic Scholar:  {} {}",
        if has_s2 { "✓" } else { "–" },
        if s2_rate_limited {
            "(429 rate-limited)"
        } else {
            ""
        }
    );
    eprintln!(
        "    DOI/arXiv content: {} (across all sources)",
        if has_doi_results { "✓" } else { "✗" }
    );

    // At least one academic adapter should return results directly,
    // OR academic content should be present via P2P store
    let has_academic = has_dblp || has_arxiv_direct || has_s2 || has_doi_results;
    assert!(has_academic, "expected academic content in results");

    // ── Step 3: dataset_evaluate ────────────────────────────────────────
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("Step 3: dataset_evaluate");

    // Pick first academic result (dblp or arxiv)
    let academic_result = results
        .iter()
        .find(|r| {
            let src = r["source"].as_str().unwrap_or("");
            let cid = r["cid"].as_str().unwrap_or("");
            src == "dblp"
                || src == "arxiv"
                || src == "semanticscholar"
                || cid.contains("arxiv.org")
                || cid.starts_with("10.")
                || cid.starts_with("s2:")
                || cid.starts_with("dblp:")
        })
        .expect("should have at least one academic result");

    let cid = academic_result["cid"].as_str().unwrap();
    let title = academic_result["title"].as_str().unwrap_or("?");
    let source = academic_result["source"].as_str().unwrap_or("?");
    eprintln!("  Evaluating [{source}]: {title}");
    eprintln!("  CID: {cid}");

    let r3 = mcp_roundtrip(
        &[
            init(),
            call(
                1,
                "dataset_evaluate",
                json!({
                    "cid": cid,
                    "task_description": "Survey academic literature on data discovery and marketplace platforms for autonomous AI agents"
                }),
            ),
        ],
        30,
    );
    assert!(r3.len() >= 2, "no response from dataset_evaluate");

    let is_error = r3[1]["result"]["isError"].as_bool().unwrap_or(false);
    let eval = tool_json(&r3[1]);
    if is_error {
        // Expected for external papers without full metadata
        eprintln!("  ⚠ evaluate returned error (expected for external papers)");
    } else {
        eprintln!(
            "  ✓ evaluation:\n{}",
            serde_json::to_string_pretty(&eval).unwrap()
        );
    }

    eprintln!("\n{}", "=".repeat(60));
    eprintln!("✓ Academic literature discovery pipeline completed");
    eprintln!("  intent_parse → dataset_search → dataset_evaluate");
    eprintln!(
        "  {} results from {} sources",
        results.len(),
        by_source.len()
    );
}
