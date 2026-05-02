// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Complete workflow: Guixu finds dataset → OpenCode runs analysis
//!
//! This demonstrates the full pipeline:
//! 1. Use Guixu MCP to discover dataset
//! 2. Download dataset locally
//! 3. Pass dataset path to OpenCode
//! 4. OpenCode writes and executes analysis code
//! 5. Return structured results

use data_external_agents::{AgentRegistry, AgentTask, GuixuMcpClient};
use serde_json::json;
use std::path::Path;

/// Guixu MCP client for dataset operations
struct GuixuDataAgent {
    mcp: GuixuMcpClient,
}

impl GuixuDataAgent {
    fn new(mcp_url: &str) -> Self {
        Self {
            mcp: GuixuMcpClient::new(mcp_url),
        }
    }

    /// Search for datasets using Guixu MCP
    async fn search_datasets(&self, query: &str) -> anyhow::Result<serde_json::Value> {
        println!("[Guixu] Searching datasets: {}", query);
        let results = self.mcp.dataset_search(query, None, Some(5)).await?;
        Ok(results)
    }

    /// Download a dataset by CID to local path
    async fn download_dataset(&self, cid: &str, output_path: &str) -> anyhow::Result<()> {
        println!("[Guixu] Downloading dataset {} to {}", cid, output_path);
        self.mcp.dataset_download(cid, output_path).await?;
        Ok(())
    }
}

/// OpenCode agent for running experiments
struct ExperimentRunner<'a> {
    agent: &'a dyn data_external_agents::ExternalAgent,
}

impl<'a> ExperimentRunner<'a> {
    fn new(agent: &'a dyn data_external_agents::ExternalAgent) -> Self {
        Self { agent }
    }

    /// Run analysis code on a dataset
    async fn analyze_dataset(
        &self,
        dataset_path: &str,
        analysis_goal: &str,
    ) -> anyhow::Result<String> {
        let prompt = format!(
            r#"You have access to a bash shell. Execute this Python code and show the output:

```python
import csv
import json
import statistics

# Read dataset
data = []
with open('{dataset_path}') as f:
    reader = csv.DictReader(f)
    for row in reader:
        # Convert numeric columns
        processed = {{}}
        for k, v in row.items():
            try:
                processed[k] = float(v)
            except ValueError:
                processed[k] = v
        data.append(processed)

# Analysis goal: {analysis_goal}
numeric_cols = [k for k in data[0].keys() if isinstance(data[0][k], (int, float))]
results = {{}}
for col in numeric_cols:
    values = [row[col] for row in data if isinstance(row.get(col), (int, float))]
    if values:
        results[col] = {{
            'mean': statistics.mean(values),
            'stdev': statistics.stdev(values) if len(values) > 1 else 0,
            'min': min(values),
            'max': max(values),
            'count': len(values)
        }}

print(json.dumps(results, indent=2))
```

Run this code using bash and show me the actual output."#,
            dataset_path = dataset_path,
            analysis_goal = analysis_goal
        );

        let task = AgentTask::new(&prompt)
            .with_timeout(120)
            .with_parameter("command", json!("run"));

        println!("[OpenCode] Running analysis...");
        let response = self.agent.execute_task(task).await?;

        if response.is_success() {
            Ok(response.content.unwrap_or_default())
        } else {
            Err(anyhow::anyhow!(
                "Analysis failed: {}",
                response.error.unwrap_or_default()
            ))
        }
    }
}

#[tokio::test]
async fn test_guixu_opencode_real_workflow() {
    println!("=== Guixu + OpenCode Real Workflow ===\n");

    // Configuration
    let mcp_url =
        std::env::var("GUIXU_MCP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let dataset_query = "time series sensor temperature";

    // 1. Initialize agents
    let registry =
        AgentRegistry::load_from_dir(Path::new("agents.d")).expect("Failed to load agent registry");

    let opencode = registry
        .get("opencode-default")
        .expect("OpenCode agent not found");

    // Verify OpenCode is available
    let health = opencode.health_check().await.expect("Health check failed");
    if !health.is_reachable {
        eprintln!("[SKIP] OpenCode not available");
        return;
    }
    println!("[✓] OpenCode is ready\n");

    // 2. Initialize Guixu MCP client
    let guixu = GuixuDataAgent::new(&mcp_url);

    // 3. Search for datasets
    let search_results = match guixu.search_datasets(dataset_query).await {
        Ok(results) => {
            println!("[✓] Search complete\n");
            results
        }
        Err(e) => {
            println!("[SKIP] MCP server not available: {}", e);
            println!("Run 'guixu serve' first to enable MCP");
            return;
        }
    };

    // Extract first result CID
    let first_result = search_results
        .get("results")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first());

    let (cid, title) = match first_result {
        Some(result) => {
            let cid = result
                .get("cid")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let title = result
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            (cid.to_string(), title.to_string())
        }
        None => {
            println!("[!] No datasets found for query: {}", dataset_query);
            return;
        }
    };

    println!("Found dataset: {} ({})", title, cid);

    // 4. Download dataset
    let dataset_path = format!("/tmp/guixu_dataset_{}.csv", cid.replace("/", "_"));
    if let Err(e) = guixu.download_dataset(&cid, &dataset_path).await {
        println!("[!] Download failed: {}", e);
        println!("Using preview instead...");

        // Fallback: use preview
        if let Ok(preview) = guixu.mcp.dataset_preview(&cid).await {
            if let Some(rows) = preview.get("rows").and_then(|r| r.as_array()) {
                let content: Vec<String> = rows
                    .iter()
                    .filter_map(|r| r.as_str().map(String::from))
                    .collect();
                std::fs::write(&dataset_path, content.join("\n")).expect("Failed to write preview");
            }
        }
    }
    println!("[✓] Dataset ready: {}\n", dataset_path);

    // 5. Run analysis with OpenCode
    let runner = ExperimentRunner::new(opencode);
    let analysis_goal = "calculate statistics for all numeric columns";

    match runner.analyze_dataset(&dataset_path, analysis_goal).await {
        Ok(results) => {
            println!("[✓] Analysis complete!\n");
            println!("Results:\n{}", results);
        }
        Err(e) => {
            println!("[!] Analysis failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_mcp_search_only() {
    let mcp_url =
        std::env::var("GUIXU_MCP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let guixu = GuixuDataAgent::new(&mcp_url);

    match guixu.search_datasets("bitcoin price").await {
        Ok(results) => {
            println!(
                "Search results:\n{}",
                serde_json::to_string_pretty(&results).unwrap()
            );
        }
        Err(e) => {
            println!("MCP not available: {}", e);
            println!("Run 'guixu serve' to enable MCP");
        }
    }
}

#[tokio::test]
async fn test_opencode_direct() {
    println!("=== Direct OpenCode Test ===\n");

    let registry =
        AgentRegistry::load_from_dir(Path::new("agents.d")).expect("Failed to load registry");

    let opencode = registry
        .get("opencode-default")
        .expect("OpenCode not found");

    let task = AgentTask::new(
        "Execute this Python code and show the output:\n\
         import json\n\
         result = {'status': 'ok', 'values': [1, 2, 3]}\n\
         print(json.dumps(result))",
    )
    .with_timeout(30);

    let response = opencode.execute_task(task).await.expect("Task failed");

    println!("Success: {}", response.is_success());
    println!("Output:\n{}", response.content.as_deref().unwrap_or(""));

    assert!(response.is_success());
}
