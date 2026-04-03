// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

const AGENTS_BEGIN: &str = "<!-- BEGIN GUIXU MCP -->";
const AGENTS_END: &str = "<!-- END GUIXU MCP -->";

const AGENTS_BLOCK: &str = r#"Use the Guixu MCP server by default for dataset acquisition workflows.

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

Do not use Guixu MCP when the task is purely about local code changes, refactoring, formatting, UI polish, or debugging unrelated to data selection or data procurement, unless the user explicitly asks to use MCP."#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    Codex,
    Cursor,
    ClaudeCode,
    OpenCode,
}

impl Client {
    pub const ALL: &[Client] = &[
        Client::Codex,
        Client::Cursor,
        Client::ClaudeCode,
        Client::OpenCode,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().replace(['-', '_'], "").as_str() {
            "codex" => Some(Self::Codex),
            "cursor" => Some(Self::Cursor),
            "claudecode" | "claude" => Some(Self::ClaudeCode),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }

    fn config_path(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        Some(match self {
            Self::Codex => home.join(".codex/config.toml"),
            Self::Cursor => home.join(".cursor/mcp.json"),
            Self::ClaudeCode => home.join(".claude.json"),
            Self::OpenCode => home.join(".config/opencode/opencode.json"),
        })
    }

    pub fn is_detected(&self) -> bool {
        match self {
            Self::ClaudeCode => which("claude").is_some(),
            _ => self
                .config_path()
                .and_then(|p| p.parent().map(|d| d.exists()))
                .unwrap_or(false),
        }
    }
}

impl fmt::Display for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codex => write!(f, "codex"),
            Self::Cursor => write!(f, "cursor"),
            Self::ClaudeCode => write!(f, "claude-code"),
            Self::OpenCode => write!(f, "opencode"),
        }
    }
}

fn guixu_bin() -> Result<String> {
    std::env::current_exe()?
        .to_str()
        .map(String::from)
        .context("non-UTF-8 executable path")
}

fn which(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let full = dir.join(cmd);
            full.is_file().then_some(full)
        })
    })
}

// ── Install ──────────────────────────────────────────────────────────────────

pub fn install(client: Client) -> Result<()> {
    let bin = guixu_bin()?;
    match client {
        Client::Codex => install_codex(&bin),
        Client::Cursor => install_json_mcp_servers(client, &bin),
        Client::ClaudeCode => install_claude_code(&bin),
        Client::OpenCode => install_opencode(client, &bin),
    }
}

/// Cursor: standard `{ "mcpServers": { "guixu": { "command", "args" } } }`
fn install_json_mcp_servers(client: Client, bin: &str) -> Result<()> {
    let path = client.config_path().context("cannot determine home dir")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut root: serde_json::Value = if path.exists() {
        serde_json::from_str(&fs::read_to_string(&path)?)?
    } else {
        serde_json::json!({})
    };

    root.as_object_mut()
        .context("config is not a JSON object")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("mcpServers not object")?
        .insert(
            "guixu".into(),
            serde_json::json!({ "command": bin, "args": ["mcp"] }),
        );

    fs::write(&path, serde_json::to_string_pretty(&root)? + "\n")?;
    println!("✅ {client} configured");
    println!("   Config: {}", path.display());
    Ok(())
}

/// Codex: TOML `[mcp_servers.guixu]` in `~/.codex/config.toml`
fn install_codex(bin: &str) -> Result<()> {
    let path = Client::Codex
        .config_path()
        .context("cannot determine home dir")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut content = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };

    content = remove_toml_section(&content, "mcp_servers.guixu");

    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    let escaped = bin.replace('\\', "\\\\").replace('"', "\\\"");
    content.push_str(&format!(
        "\n[mcp_servers.guixu]\ncommand = \"{escaped}\"\nargs = [\"mcp\", \"--mode\", \"codex\"]\n"
    ));

    fs::write(&path, &content)?;
    write_agents_md()?;

    println!("✅ codex configured");
    println!("   Config: {}", path.display());
    Ok(())
}

/// Claude Code: prefer `claude mcp add` CLI, fall back to editing `~/.claude.json`
fn install_claude_code(bin: &str) -> Result<()> {
    if let Some(claude) = which("claude") {
        let status = Command::new(claude)
            .args([
                "mcp",
                "add",
                "--transport",
                "stdio",
                "--scope",
                "user",
                "guixu",
                "--",
                bin,
                "mcp",
            ])
            .status();
        if let Ok(s) = status {
            if s.success() {
                println!("✅ claude-code configured (via `claude mcp add`)");
                return Ok(());
            }
        }
    }

    // Fallback: edit ~/.claude.json directly
    let path = Client::ClaudeCode
        .config_path()
        .context("cannot determine home dir")?;

    let mut root: serde_json::Value = if path.exists() {
        serde_json::from_str(&fs::read_to_string(&path)?)?
    } else {
        serde_json::json!({})
    };

    root.as_object_mut()
        .context("config is not a JSON object")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("mcpServers not object")?
        .insert(
            "guixu".into(),
            serde_json::json!({ "type": "stdio", "command": bin, "args": ["mcp"] }),
        );

    fs::write(&path, serde_json::to_string_pretty(&root)? + "\n")?;
    println!("✅ claude-code configured");
    println!("   Config: {}", path.display());
    Ok(())
}

/// OpenCode: `{ "mcp": { "guixu": { "type": "local", "command": [...], "enabled": true } } }`
fn install_opencode(client: Client, bin: &str) -> Result<()> {
    let path = client.config_path().context("cannot determine home dir")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut root: serde_json::Value = if path.exists() {
        serde_json::from_str(&fs::read_to_string(&path)?)?
    } else {
        serde_json::json!({})
    };

    root.as_object_mut()
        .context("config is not a JSON object")?
        .entry("mcp")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("mcp not object")?
        .insert(
            "guixu".into(),
            serde_json::json!({
                "type": "local",
                "command": [bin, "mcp"],
                "enabled": true
            }),
        );

    fs::write(&path, serde_json::to_string_pretty(&root)? + "\n")?;
    println!("✅ {client} configured");
    println!("   Config: {}", path.display());
    Ok(())
}

fn remove_toml_section(content: &str, section: &str) -> String {
    let header = format!("[{section}]");
    let mut out = String::new();
    let mut skip = false;
    for line in content.lines() {
        if line.trim() == header {
            skip = true;
            continue;
        }
        if skip && line.starts_with('[') {
            skip = false;
        }
        if !skip {
            out.push_str(line);
            out.push('\n');
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

fn write_agents_md() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let is_project = [".git", "Cargo.toml", "package.json"]
        .iter()
        .any(|f| cwd.join(f).exists());
    if !is_project {
        return Ok(());
    }

    let path = cwd.join("AGENTS.md");
    let mut content = if path.exists() {
        strip_between(&fs::read_to_string(&path)?, AGENTS_BEGIN, AGENTS_END)
    } else {
        "# AGENTS.md\n".to_string()
    };

    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("\n{AGENTS_BEGIN}\n{AGENTS_BLOCK}\n{AGENTS_END}\n"));
    fs::write(&path, &content)?;
    println!("   Agents: {}", path.display());
    Ok(())
}

fn strip_between(content: &str, begin: &str, end: &str) -> String {
    let mut out = String::new();
    let mut skip = false;
    for line in content.lines() {
        if line.trim() == begin {
            skip = true;
            continue;
        }
        if skip && line.trim() == end {
            skip = false;
            continue;
        }
        if !skip {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

// ── Uninstall ────────────────────────────────────────────────────────────────

pub fn uninstall(client: Client) -> Result<()> {
    match client {
        Client::Codex => uninstall_codex(),
        Client::ClaudeCode => uninstall_claude_code(),
        Client::OpenCode => uninstall_opencode_entry(client),
        Client::Cursor => uninstall_json_mcp_servers(client),
    }
}

fn uninstall_json_mcp_servers(client: Client) -> Result<()> {
    let path = client.config_path().context("cannot determine home dir")?;
    if !path.exists() {
        println!("Nothing to remove — {} does not exist.", path.display());
        return Ok(());
    }
    let mut root: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
    if let Some(servers) = root.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        servers.remove("guixu");
    }
    fs::write(&path, serde_json::to_string_pretty(&root)? + "\n")?;
    println!("✅ {client} — guixu MCP removed");
    Ok(())
}

fn uninstall_codex() -> Result<()> {
    let path = Client::Codex
        .config_path()
        .context("cannot determine home dir")?;
    if !path.exists() {
        println!("Nothing to remove.");
        return Ok(());
    }
    let content = fs::read_to_string(&path)?;
    fs::write(&path, remove_toml_section(&content, "mcp_servers.guixu"))?;
    println!("✅ codex — guixu MCP removed");
    Ok(())
}

fn uninstall_claude_code() -> Result<()> {
    if let Some(claude) = which("claude") {
        let status = Command::new(claude)
            .args(["mcp", "remove", "guixu"])
            .status();
        if let Ok(s) = status {
            if s.success() {
                println!("✅ claude-code — guixu MCP removed (via `claude mcp remove`)");
                return Ok(());
            }
        }
    }
    // Fallback
    uninstall_json_mcp_servers(Client::ClaudeCode)
}

fn uninstall_opencode_entry(client: Client) -> Result<()> {
    let path = client.config_path().context("cannot determine home dir")?;
    if !path.exists() {
        println!("Nothing to remove — {} does not exist.", path.display());
        return Ok(());
    }
    let mut root: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
    if let Some(mcp) = root.get_mut("mcp").and_then(|v| v.as_object_mut()) {
        mcp.remove("guixu");
    }
    fs::write(&path, serde_json::to_string_pretty(&root)? + "\n")?;
    println!("✅ {client} — guixu MCP removed");
    Ok(())
}

// ── List ─────────────────────────────────────────────────────────────────────

pub fn list_detected() {
    println!("Supported clients:");
    for c in Client::ALL {
        let tag = if c.is_detected() { "detected" } else { "—" };
        println!("  {c:<14} {tag}");
    }
}
