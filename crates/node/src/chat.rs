// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Interactive chat REPL for Guixu Agent.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Write};

use data_core::config::NodeConfig;
use data_core::identity::NodeIdentity;
use data_mcp_server::server::AppState;
use data_storage::feedback_store::FeedbackStore;
use data_storage::job_store::JobStore;
use data_storage::metadata_store::MetadataStore;

/// LLM provider configuration.
struct LlmConfig {
    provider: String,
    model: String,
    api_key: String,
    api_base: String,
}

/// Chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Tool definition for function calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolDef {
    name: String,
    description: String,
    parameters: Value,
}

/// Run the interactive chat REPL.
pub async fn run(
    provider: String,
    model: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> Result<()> {
    // Load config
    let config = LlmConfig::new(provider, model, api_key, api_base)?;

    // Initialize MCP state
    let state = build_mcp_state().await?;
    let tools = build_tool_definitions();

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║           Guixu Agent - Interactive Chat                ║");
    println!("╠═══════════════════════════════════════════════════════════╣");
    println!("║  Provider: {:<46} ║", config.provider);
    println!("║  Model:    {:<46} ║", config.model);
    println!("║                                                         ║");
    println!("║  Commands: /help /quit /clear /tools /history           ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    let mut messages: Vec<ChatMessage> = Vec::new();
    let client = Client::new();

    loop {
        print!("You> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        // Handle commands
        match input {
            "/quit" | "/exit" | "/q" => {
                println!("Goodbye!");
                break;
            }
            "/help" | "/h" => {
                print_help();
                continue;
            }
            "/clear" | "/c" => {
                messages.clear();
                println!("Conversation cleared.");
                continue;
            }
            "/tools" | "/t" => {
                print_tools(&tools);
                continue;
            }
            "/history" => {
                print_history(&messages);
                continue;
            }
            _ => {}
        }

        // Add user message
        messages.push(ChatMessage {
            role: "user".into(),
            content: input.to_string(),
        });

        // Call LLM
        match call_llm(&client, &config, &messages, &tools).await {
            Ok(response) => {
                // Process response
                let assistant_msg = process_response(&response, &state, &mut messages).await?;
                println!("\nAgent> {}\n", assistant_msg);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                messages.pop(); // Remove failed user message
            }
        }
    }

    Ok(())
}

impl LlmConfig {
    fn new(
        provider: String,
        model: Option<String>,
        api_key: Option<String>,
        api_base: Option<String>,
    ) -> Result<Self> {
        let api_key = api_key
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .or_else(|| std::env::var("OLLAMA_API_KEY").ok())
            .unwrap_or_default();

        let (default_model, default_base) = match provider.as_str() {
            "openai" => ("gpt-4".into(), "https://api.openai.com/v1".into()),
            "anthropic" => (
                "claude-3-sonnet-20240229".into(),
                "https://api.anthropic.com".into(),
            ),
            "ollama" => ("llama3".into(), "http://localhost:11434".into()),
            _ => anyhow::bail!(
                "Unknown provider: {}. Use openai, anthropic, or ollama.",
                provider
            ),
        };

        Ok(Self {
            provider,
            model: model.unwrap_or(default_model),
            api_key,
            api_base: api_base.unwrap_or(default_base),
        })
    }
}

/// Build MCP state for tool execution.
async fn build_mcp_state() -> Result<AppState> {
    let config = NodeConfig::load_or_default();
    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let feedback_store = FeedbackStore::open(&NodeConfig::config_dir().join("feedback_db"))?;
    let job_store = JobStore::open(&NodeConfig::config_dir().join("job_db"))?;

    // Create a dummy network handle for standalone mode
    let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::channel(8);
    let dht = data_p2p::dht::DhtIndex::new(data_p2p::network::NetworkHandle {
        cmd_tx,
        local_peer_id: libp2p::PeerId::random(),
    });

    Ok(AppState::with_full_config(
        NodeIdentity::generate(),
        dht,
        store,
        feedback_store,
        job_store,
        &config.payment,
        &[],
        &[],
        &[],
    )
    .await)
}

/// Build tool definitions for LLM function calling.
fn build_tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "intent_parse".into(),
            description: "Parse a data request into structured intent".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The user's data request" }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "dataset_search".into(),
            description: "Search datasets across registered data sources".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "description": "Max results", "default": 10 }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "dataset_list".into(),
            description: "List available datasets".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "mine": { "type": "boolean", "description": "Only show my datasets", "default": false }
                }
            }),
        },
        ToolDef {
            name: "dataset_preview".into(),
            description: "Preview a dataset".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string", "description": "Dataset CID" },
                    "rows": { "type": "integer", "description": "Number of rows", "default": 10 }
                },
                "required": ["cid"]
            }),
        },
    ]
}

/// Call LLM API.
async fn call_llm(
    client: &Client,
    config: &LlmConfig,
    messages: &[ChatMessage],
    tools: &[ToolDef],
) -> Result<Value> {
    match config.provider.as_str() {
        "openai" | "ollama" => call_openai_compatible(client, config, messages, tools).await,
        "anthropic" => call_anthropic(client, config, messages, tools).await,
        _ => anyhow::bail!("Unsupported provider"),
    }
}

/// Call OpenAI-compatible API.
async fn call_openai_compatible(
    client: &Client,
    config: &LlmConfig,
    messages: &[ChatMessage],
    tools: &[ToolDef],
) -> Result<Value> {
    let url = format!("{}/chat/completions", config.api_base);

    let tools_json: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            })
        })
        .collect();

    let body = json!({
        "model": config.model,
        "messages": messages,
        "tools": tools_json,
        "tool_choice": "auto"
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Failed to call LLM API")?;

    let result: Value = response.json().await?;
    Ok(result)
}

/// Call Anthropic API.
async fn call_anthropic(
    client: &Client,
    config: &LlmConfig,
    messages: &[ChatMessage],
    tools: &[ToolDef],
) -> Result<Value> {
    let url = format!("{}/v1/messages", config.api_base);

    let tools_json: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters
            })
        })
        .collect();

    // Convert messages to Anthropic format
    let system_msg = messages
        .iter()
        .find(|m| m.role == "system")
        .map(|m| m.content.clone())
        .unwrap_or_default();

    let chat_messages: Vec<Value> = messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();

    let body = json!({
        "model": config.model,
        "max_tokens": 4096,
        "system": system_msg,
        "messages": chat_messages,
        "tools": tools_json
    });

    let response = client
        .post(&url)
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Failed to call Anthropic API")?;

    let result: Value = response.json().await?;
    Ok(result)
}

/// Process LLM response and execute tools if needed.
async fn process_response(
    response: &Value,
    state: &AppState,
    messages: &mut Vec<ChatMessage>,
) -> Result<String> {
    // Extract message content
    let (content, tool_calls) = extract_response(response)?;

    // Add assistant message
    messages.push(ChatMessage {
        role: "assistant".into(),
        content: content.clone(),
    });

    // Execute tool calls if any
    if let Some(calls) = tool_calls {
        let mut tool_results = Vec::new();
        let num_calls = calls.len();

        for call in &calls {
            let tool_name = call["function"]["name"].as_str().unwrap_or("");
            let tool_args = call["function"]["arguments"].clone();

            println!("  [Calling tool: {}]", tool_name);

            let result = execute_tool(state, tool_name, tool_args).await?;
            tool_results.push(json!({
                "tool_call_id": call["id"],
                "role": "tool",
                "content": result
            }));
        }

        // Add tool results and call LLM again
        for result in tool_results {
            messages.push(ChatMessage {
                role: "tool".into(),
                content: result["content"].as_str().unwrap_or("").to_string(),
            });
        }

        // Return tool execution summary
        return Ok(format!("{}\n[Executed {} tool(s)]", content, num_calls));
    }

    Ok(content)
}

/// Extract content and tool calls from LLM response.
fn extract_response(response: &Value) -> Result<(String, Option<Vec<Value>>)> {
    // OpenAI format
    if let Some(choices) = response["choices"].as_array() {
        if let Some(choice) = choices.first() {
            let message = &choice["message"];
            let content = message["content"].as_str().unwrap_or("").to_string();
            let tool_calls = message["tool_calls"].as_array().map(|arr| arr.to_vec());
            return Ok((content, tool_calls));
        }
    }

    // Anthropic format
    if let Some(content) = response["content"].as_array() {
        let text: String = content
            .iter()
            .filter_map(|c| {
                if c["type"] == "text" {
                    c["text"].as_str().map(String::from)
                } else {
                    None
                }
            })
            .collect();

        let tool_use: Vec<Value> = content
            .iter()
            .filter(|c| c["type"] == "tool_use")
            .cloned()
            .collect();

        if tool_use.is_empty() {
            Ok((text, None))
        } else {
            Ok((text, Some(tool_use)))
        }
    } else {
        anyhow::bail!("Unexpected response format: {}", response)
    }
}

/// Execute a tool call.
async fn execute_tool(state: &AppState, name: &str, args: Value) -> Result<String> {
    // This is a simplified version - in production, you'd call the actual MCP handlers
    match name {
        "dataset_list" => {
            let store = &state.store;
            let datasets = store.list_all()?;
            let mine = args["mine"].as_bool().unwrap_or(false);

            let items: Vec<Value> = datasets
                .iter()
                .filter(|d| !mine || d.provider.0 == state.identity.did.0)
                .map(|d| {
                    json!({
                        "cid": d.cid.0,
                        "title": d.title,
                        "rows": d.schema.row_count,
                        "size": d.schema.size_bytes
                    })
                })
                .collect();

            Ok(serde_json::to_string_pretty(&items)?)
        }
        "dataset_preview" => {
            let cid = args["cid"].as_str().unwrap_or("");
            let _rows = args["rows"].as_u64().unwrap_or(10) as usize;

            let store = &state.store;
            let dataset_cid = data_core::types::DatasetCid(cid.to_string());

            match store.get(&dataset_cid)? {
                Some(metadata) => Ok(format!(
                    "Dataset: {}\nRows: {}\nColumns: {}\nSize: {} bytes",
                    metadata.title,
                    metadata.schema.row_count,
                    metadata.schema.columns.len(),
                    metadata.schema.size_bytes
                )),
                None => Ok(format!("Dataset not found: {}", cid)),
            }
        }
        _ => Ok(format!("Tool '{}' executed (demo mode)", name)),
    }
}

/// Print help message.
fn print_help() {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║                     Available Commands                  ║");
    println!("╠═══════════════════════════════════════════════════════════╣");
    println!("║  /help, /h      - Show this help message               ║");
    println!("║  /quit, /q      - Exit the chat                        ║");
    println!("║  /clear, /c     - Clear conversation history           ║");
    println!("║  /tools, /t     - List available tools                 ║");
    println!("║  /history       - Show conversation history            ║");
    println!("╠═══════════════════════════════════════════════════════════╣");
    println!("║                     Example Queries                     ║");
    println!("╠═══════════════════════════════════════════════════════════╣");
    println!("║  \"Search for climate data\"                             ║");
    println!("║  \"Find datasets about stock prices\"                   ║");
    println!("║  \"List my published datasets\"                         ║");
    println!("║  \"Preview dataset Qm...\"                              ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
}

/// Print available tools.
fn print_tools(tools: &[ToolDef]) {
    println!("Available tools:");
    for tool in tools {
        println!("  - {}: {}", tool.name, tool.description);
    }
}

/// Print conversation history.
fn print_history(messages: &[ChatMessage]) {
    if messages.is_empty() {
        println!("No conversation history.");
        return;
    }

    println!("Conversation history:");
    for msg in messages {
        let prefix = match msg.role.as_str() {
            "user" => "You",
            "assistant" => "Agent",
            "system" => "System",
            "tool" => "Tool",
            _ => &msg.role,
        };
        println!("  {}: {}", prefix, truncate(&msg.content, 80));
    }
}

/// Truncate string to max length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
