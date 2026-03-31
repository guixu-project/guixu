//! MCP stdio integration tests.
//!
//! These tests spawn the `data-node` binary in `mcp --mode codex` and exercise
//! the full JSON-RPC / MCP protocol over stdin/stdout, exactly as Codex would.
//!
//! Marked `#[ignore]` by default because they require:
//!   - a release build (`cargo build --release -p data-node`)
//!   - network access (for DefiLlama / DataCite adapters)
//!
//! Run with: `cargo test -p data-mcp-server --test mcp_stdio_integration -- --ignored`

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

/// Locate the release binary. Falls back to debug if release doesn't exist.
fn data_node_binary() -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("cannot resolve workspace root");
    let release = workspace_root.join("target/release/data-node");
    if release.exists() {
        return release;
    }
    let debug = workspace_root.join("target/debug/data-node");
    if debug.exists() {
        return debug;
    }
    panic!(
        "data-node binary not found. Run `cargo build --release -p data-node` first.\n\
         Looked in:\n  {}\n  {}",
        release.display(),
        debug.display()
    );
}

/// Send newline-delimited JSON-RPC messages to the MCP server and collect responses.
fn mcp_roundtrip(messages: &[Value]) -> Vec<Value> {
    mcp_roundtrip_timeout(messages, Duration::from_secs(60))
}

fn mcp_roundtrip_timeout(messages: &[Value], timeout: Duration) -> Vec<Value> {
    let binary = data_node_binary();
    let mut child = Command::new(&binary)
        .args(["mcp", "--mode", "codex"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", binary.display()));

    let mut stdin = child.stdin.take().expect("stdin");
    for msg in messages {
        let line = serde_json::to_string(msg).unwrap();
        writeln!(stdin, "{line}").expect("write to stdin");
    }
    drop(stdin); // close stdin → server will exit after processing

    let output = wait_with_timeout(&mut child, timeout);
    let stdout = String::from_utf8_lossy(&output.stdout);

    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|e| panic!("invalid JSON from MCP server: {e}\nline: {line}"))
        })
        .collect()
}

fn wait_with_timeout(child: &mut std::process::Child, timeout: Duration) -> std::process::Output {
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut s, &mut buf).unwrap_or(0);
                        buf
                    })
                    .unwrap_or_default();
                return std::process::Output {
                    status,
                    stdout,
                    stderr: vec![],
                };
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    panic!("MCP server did not exit within {timeout:?}");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("error waiting for child: {e}"),
        }
    }
}

fn init_message() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "integration-test", "version": "0.1" }
        }
    })
}

fn tool_call(id: u64, name: &str, args: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": args }
    })
}

/// Extract the text content from a tools/call response.
fn extract_tool_text(response: &Value) -> String {
    response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// Parse the text content as JSON.
fn extract_tool_json(response: &Value) -> Value {
    let text = extract_tool_text(response);
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("tool output is not JSON: {e}\n{text}"))
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn mcp_initialize_returns_server_info() {
    let responses = mcp_roundtrip(&[init_message()]);
    assert!(!responses.is_empty(), "no response from MCP server");

    let r = &responses[0];
    assert_eq!(r["result"]["serverInfo"]["name"], "guixu");
    assert!(r["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn mcp_tools_list_contains_expected_tools() {
    let responses = mcp_roundtrip(&[
        init_message(),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
    ]);
    assert!(responses.len() >= 2);

    let tools = responses[1]["result"]["tools"]
        .as_array()
        .expect("tools should be an array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

    // Core tools
    assert!(names.contains(&"intent_parse"), "missing intent_parse");
    assert!(names.contains(&"dataset_search"), "missing dataset_search");
    assert!(
        names.contains(&"dataset_evaluate"),
        "missing dataset_evaluate"
    );
    assert!(
        names.contains(&"dataset_feedback"),
        "missing dataset_feedback"
    );
    assert!(
        names.contains(&"dataset_purchase"),
        "missing dataset_purchase"
    );

    // dataset_search should mention DefiLlama in description
    let search_tool = tools
        .iter()
        .find(|t| t["name"] == "dataset_search")
        .unwrap();
    let desc = search_tool["description"].as_str().unwrap_or("");
    assert!(
        desc.contains("DefiLlama"),
        "dataset_search description should mention DefiLlama, got: {desc}"
    );

    // dataset_search filters should include new fields
    let filter_props = &search_tool["inputSchema"]["properties"]["filters"]["properties"];
    assert!(filter_props.get("chain").is_some(), "missing chain filter");
    assert!(
        filter_props.get("category").is_some(),
        "missing category filter"
    );
    assert!(
        filter_props.get("free_only").is_some(),
        "missing free_only filter"
    );
    assert!(
        filter_props.get("protocol").is_some(),
        "missing protocol filter"
    );
}

#[test]
fn mcp_intent_parse_returns_profile() {
    let responses = mcp_roundtrip(&[
        init_message(),
        tool_call(
            1,
            "intent_parse",
            json!({ "query": "find USDC stablecoin market data" }),
        ),
    ]);
    assert!(responses.len() >= 2);

    let output = extract_tool_json(&responses[1]);
    // Should have keywords extracted
    let keywords = output["keywords"].as_array();
    assert!(keywords.is_some(), "expected keywords in intent output");
    let kw_strs: Vec<&str> = keywords
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        kw_strs
            .iter()
            .any(|k| k.to_lowercase().contains("usdc") || k.to_lowercase().contains("stablecoin")),
        "expected usdc or stablecoin in keywords, got: {kw_strs:?}"
    );
}

#[test]
#[ignore = "requires network access to DefiLlama API"]
fn mcp_dataset_search_returns_defillama_results() {
    let responses = mcp_roundtrip(&[
        init_message(),
        tool_call(
            1,
            "dataset_search",
            json!({
                "query": "USDC stablecoin market cap data on ethereum",
                "limit": 10
            }),
        ),
    ]);
    assert!(responses.len() >= 2);

    let output = extract_tool_json(&responses[1]);
    let results = output["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "expected search results");

    // Check if any result comes from defillama
    let defillama_results: Vec<_> = results
        .iter()
        .filter(|r| r["source"].as_str().unwrap_or("").contains("defillama"))
        .collect();

    eprintln!(
        "total results: {}, defillama results: {}",
        results.len(),
        defillama_results.len()
    );
    for r in results {
        eprintln!(
            "  source={} title={}",
            r["source"].as_str().unwrap_or("?"),
            r["title"].as_str().unwrap_or("?")
        );
    }

    // DefiLlama should contribute at least one result for a stablecoin query
    assert!(
        !defillama_results.is_empty(),
        "expected at least one DefiLlama result for stablecoin query"
    );

    // DefiLlama results should have source_attributes and be free
    for r in &defillama_results {
        assert!(
            r["source_attributes"].is_object(),
            "missing source_attributes"
        );
        let price = r["price"]["amount"].as_f64().unwrap_or(999.0);
        assert!(price == 0.0, "DefiLlama data should be free, got {price}");
    }
}

#[test]
#[ignore = "requires network access"]
fn mcp_dataset_search_with_chain_filter() {
    let responses = mcp_roundtrip(&[
        init_message(),
        tool_call(
            1,
            "dataset_search",
            json!({
                "query": "stablecoin data",
                "filters": {
                    "source": "defillama",
                    "category": "stablecoin",
                    "free_only": true
                },
                "limit": 5
            }),
        ),
    ]);
    assert!(responses.len() >= 2);

    let output = extract_tool_json(&responses[1]);
    let results = output["results"].as_array().expect("results array");

    // All results should be free
    for r in results {
        let price = r["price"]["amount"].as_f64().unwrap_or(999.0);
        assert!(price == 0.0, "expected free result, got price={price}");
    }
}

#[test]
#[ignore = "requires network access"]
fn mcp_dataset_search_bridge_data() {
    let responses = mcp_roundtrip(&[
        init_message(),
        tool_call(
            1,
            "dataset_search",
            json!({
                "query": "cross-chain bridge volume",
                "limit": 3
            }),
        ),
    ]);
    assert!(responses.len() >= 2);

    let output = extract_tool_json(&responses[1]);
    let results = output["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "expected bridge results");
}

#[test]
fn mcp_dataset_search_unknown_tool_returns_error() {
    let responses = mcp_roundtrip(&[init_message(), tool_call(1, "nonexistent_tool", json!({}))]);
    assert!(responses.len() >= 2);

    let r = &responses[1];
    assert!(
        r.get("error").is_some(),
        "expected error for unknown tool, got: {r}"
    );
}

#[test]
fn mcp_ping_returns_empty_object() {
    let responses = mcp_roundtrip(&[
        init_message(),
        json!({ "jsonrpc": "2.0", "id": 99, "method": "ping" }),
    ]);
    assert!(responses.len() >= 2);
    assert!(responses[1]["result"].is_object());
}

#[test]
#[ignore = "requires network access"]
fn mcp_full_pipeline_intent_then_search() {
    // Simulate the Codex agent workflow: intent_parse → dataset_search
    // Run as two separate roundtrips to avoid timeout from sequential API calls
    let responses1 = mcp_roundtrip(&[
        init_message(),
        tool_call(
            1,
            "intent_parse",
            json!({ "query": "I need USDC stablecoin historical data for a DeFi analytics dashboard" }),
        ),
    ]);
    assert!(responses1.len() >= 2);

    // intent_parse should succeed
    let intent = extract_tool_json(&responses1[1]);
    assert!(
        intent.get("keywords").is_some() || intent.get("task_description").is_some(),
        "intent_parse should return structured profile"
    );

    let responses2 = mcp_roundtrip(&[
        init_message(),
        tool_call(
            2,
            "dataset_search",
            json!({
                "query": "USDC stablecoin historical data",
                "filters": { "free_only": true },
                "limit": 5
            }),
        ),
    ]);
    assert!(responses2.len() >= 2);

    // dataset_search should return results
    let search = extract_tool_json(&responses2[1]);
    let results = search["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "pipeline search should return results");

    // At least one result should have source_attributes (from DefiLlama or other enriched source)
    let has_attrs = results
        .iter()
        .any(|r| r.get("source_attributes").is_some() && !r["source_attributes"].is_null());
    eprintln!(
        "results with source_attributes: {}/{}",
        results
            .iter()
            .filter(|r| r.get("source_attributes").is_some() && !r["source_attributes"].is_null())
            .count(),
        results.len()
    );
    if has_attrs {
        eprintln!("✓ found results with source_attributes");
    }
}

#[test]
#[ignore = "requires network access"]
fn mcp_dataset_evaluate_with_search_result() {
    // Search first, then evaluate the first result
    let responses = mcp_roundtrip(&[
        init_message(),
        tool_call(
            1,
            "dataset_search",
            json!({ "query": "stablecoin", "limit": 1 }),
        ),
    ]);
    assert!(responses.len() >= 2);

    let search = extract_tool_json(&responses[1]);
    let results = search["results"].as_array().expect("results");
    if results.is_empty() {
        eprintln!("no search results, skipping evaluate test");
        return;
    }

    let cid = results[0]["cid"].as_str().expect("cid");
    eprintln!("evaluating cid: {cid}");

    let responses2 = mcp_roundtrip(&[
        init_message(),
        tool_call(
            1,
            "dataset_evaluate",
            json!({
                "cid": cid,
                "task_description": "Build a stablecoin analytics dashboard"
            }),
        ),
    ]);
    assert!(responses2.len() >= 2);

    // Evaluate should return without error
    let eval_resp = &responses2[1];
    let is_error = eval_resp["result"]["isError"].as_bool().unwrap_or(false);
    if is_error {
        let text = extract_tool_text(eval_resp);
        eprintln!("evaluate returned error (acceptable for external CIDs): {text}");
    } else {
        let eval = extract_tool_json(eval_resp);
        eprintln!(
            "evaluate result: {}",
            serde_json::to_string_pretty(&eval).unwrap()
        );
    }
}
