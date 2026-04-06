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
    OpenClaw,
}

impl Client {
    pub const ALL: &[Client] = &[
        Client::Codex,
        Client::Cursor,
        Client::ClaudeCode,
        Client::OpenCode,
        Client::OpenClaw,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().replace(['-', '_'], "").as_str() {
            "codex" => Some(Self::Codex),
            "cursor" => Some(Self::Cursor),
            "claudecode" | "claude" => Some(Self::ClaudeCode),
            "opencode" => Some(Self::OpenCode),
            "openclaw" => Some(Self::OpenClaw),
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
            Self::OpenClaw => home.join(".openclaw/config.json"),
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
            Self::OpenClaw => write!(f, "openclaw"),
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

fn upsert_json_mcp_server(
    root: &mut serde_json::Value,
    name: &str,
    entry: serde_json::Value,
) -> Result<()> {
    root.as_object_mut()
        .context("config is not a JSON object")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("mcpServers not object")?
        .insert(name.into(), entry);
    Ok(())
}

fn remove_json_mcp_server(root: &mut serde_json::Value, name: &str) {
    if let Some(servers) = root.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        servers.remove(name);
    }
}

const OPENCLAW_SKILL_TRIGGERS: &str = r#"## When to Use Guixu

Invoke Guixu when the task involves:
- Finding training data, test data, or benchmark datasets
- Building or evaluating a model, classifier, detector, segmenter, ranker, or retriever
- Acquiring labeled examples, curated corpora, or domain-specific data
- Comparing dataset quality, coverage, or licensing before use

Do NOT invoke Guixu for local code changes, refactoring, formatting, bug fixes, or debugging unrelated to data procurement."#;

const OPENCLAW_SKILL_WORKFLOW: &str = r#"## Standard Workflow

Always use tools in this order:

1. **intent_parse** — Parse the natural-language request into a structured profile.
   Extract task type, content keywords, data modality, and budget.

2. **dataset_search** — Search across DeFi, RWA, Kaggle, HuggingFace, IPFS,
   BitTorrent, academic indices, and the P2P network. Filter by source, price,
   license, and quality.

3. **dataset_evaluate** — Score candidate datasets by Task-Conditioned Value (TCV).
   Inputs: CID, task description, required columns, budget.

4. **dataset_purchase** — Acquire a paid dataset via smart contract payment
   (x402 / Machine Payment Protocol). For free datasets, skip to download.

5. **dataset_feedback** — Record on-chain attestation after use to help
   future agents evaluate this dataset.

## Fallback Rules

- If `dataset_search` returns no results, try relaxing filters (remove price cap,
  set `free_only: true`, or broaden keywords).
- If `dataset_evaluate` returns a negative score, treat the dataset as harmful
  and skip it.
- If `dataset_purchase` fails due to insufficient balance, fall back to
  free alternatives from `dataset_search` with `free_only: true`."#;

const OPENCLAW_SKILL_EXAMPLES: &str = r#"## Minimal Examples

### Find cat image datasets for object detection
```
intent_parse(query="Find cat images for training an object detector",
             task_type="detection",
             keywords=["cat", "image"],
             sample_unit="image")
→ returns { task_type, keywords, sample_unit }

dataset_search(query="cat object detection dataset",
               task_type="detection",
               filters={ "free_only": true })
→ returns [ { cid, source, price, license, schema } ]

dataset_evaluate(cid="ipfs:Qm...",
                 task_description="train cat detector",
                 task_type="detection",
                 required_columns=["image_path", "bbox"])
→ returns { score: 0-100 }

dataset_purchase(cid="ipfs:Qm...", max_price=5)
dataset_feedback(cid="ipfs:Qm...", relevance_score=0.9,
                 quality_rating=5, task_success=true,
                 value_assessment="positive")
```

### Acquire stock price data for time-series forecasting
```
intent_parse(query="historical US stock prices for forecasting",
             task_type="forecasting",
             keywords=["stock", "price"],
             sample_unit="tabular")
→ returns { ... }

dataset_search(query="stock OHLCV dataset",
               filters={ "chain": "ethereum", "category": "defi" })
→ returns [ { cid, source, price, license } ]

dataset_evaluate(cid="kaggle:org/project",
                 task_description="train LSTM price forecaster",
                 task_type="forecasting",
                 required_columns=["open","high","low","close","volume"])
→ returns { score: 0-100 }
```"#;

fn openclaw_skill_markdown() -> String {
    format!(
        "---\n\
         name: guixu\n\
         description: Unified data discovery and market for AI agents. Search, value, and acquire datasets on-chain.\n\
         version: 0.1.0\n\
         metadata:\n\
         \x20 openclaw:\n\
         \x20   requires:\n\
         \x20     bins:\n\
         \x20       - guixu\n\
         \x20   triggers:\n\
         \x20     - dataset\n\
         \x20     - train\n\
         \x20     - model\n\
         \x20     - data acquisition\n\
         \x20     - benchmark\n\
         \x20     - dataset search\n\
         \x20   emoji: \"🌊\"\n\
         ---\n\n\
         # Guixu — Data Discovery & Market for AI Agents\n\n\
         {AGENTS_BLOCK}\n\n\
         {OPENCLAW_SKILL_TRIGGERS}\n\n\
         {OPENCLAW_SKILL_WORKFLOW}\n\n\
         {OPENCLAW_SKILL_EXAMPLES}\n",
    )
}

// ── Install ──────────────────────────────────────────────────────────────────

pub fn install(client: Client) -> Result<()> {
    let bin = guixu_bin()?;
    match client {
        Client::Codex => install_codex(&bin),
        Client::Cursor => install_json_mcp_servers(client, &bin),
        Client::ClaudeCode => install_claude_code(&bin),
        Client::OpenCode => install_opencode(client, &bin),
        Client::OpenClaw => install_openclaw(&bin),
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

    upsert_json_mcp_server(
        &mut root,
        "guixu",
        serde_json::json!({ "command": bin, "args": ["mcp"] }),
    )?;

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

/// OpenClaw: MCP entry in `~/.openclaw/config.json` + skill in `~/.openclaw/workspace/skills/guixu/`
fn install_openclaw(bin: &str) -> Result<()> {
    let home = dirs::home_dir().context("cannot determine home dir")?;
    let config_path = home.join(".openclaw/config.json");
    let skill_dir = home.join(".openclaw/workspace/skills/guixu");

    // 1. Register MCP server in config.json
    {
        let parent = config_path.parent().expect("config.json has no parent");
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;

        let raw = if config_path.exists() {
            fs::read_to_string(&config_path).context("failed to read openclaw config")?
        } else {
            String::new()
        };

        let mut root: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                anyhow::bail!(
                    "openclaw config is not valid JSON — {}: {}\n\
                     File: {}\n\
                     Fix: back up the file and remove it, then re-run install.",
                    config_path.display(),
                    e,
                    raw
                );
            }
        };

        upsert_json_mcp_server(
            &mut root,
            "guixu",
            serde_json::json!({ "command": bin, "args": ["mcp"] }),
        )?;

        fs::write(&config_path, serde_json::to_string_pretty(&root)? + "\n")
            .context("failed to write openclaw config")?;
    }

    // 2. Install skill
    fs::create_dir_all(&skill_dir)
        .with_context(|| format!("failed to create skill directory {}", skill_dir.display()))?;
    let skill_md = openclaw_skill_markdown();
    fs::write(skill_dir.join("SKILL.md"), &skill_md).with_context(|| {
        format!(
            "failed to write skill file at {}",
            skill_dir.join("SKILL.md").display()
        )
    })?;

    println!("✅ openclaw configured");
    println!("   Config: {}", config_path.display());
    println!("   Skill:  {}", skill_dir.join("SKILL.md").display());
    println!();
    println!("To verify:");
    println!("  1. Open openclaw and run: intent_parse(query=\"find cat images\", task_type=\"detection\", keywords=[\"cat\"], sample_unit=\"image\")");
    println!("  2. Check that MCP server 'guixu' is listed in openclaw's active servers");
    println!("  3. If not detected, restart openclaw or check ~/.openclaw/config.json");

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
        Client::OpenClaw => uninstall_openclaw(),
    }
}

fn uninstall_json_mcp_servers(client: Client) -> Result<()> {
    let path = client.config_path().context("cannot determine home dir")?;
    if !path.exists() {
        println!("Nothing to remove — {} does not exist.", path.display());
        return Ok(());
    }
    let mut root: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
    remove_json_mcp_server(&mut root, "guixu");
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

fn uninstall_openclaw() -> Result<()> {
    let home = dirs::home_dir().context("cannot determine home dir")?;
    let config_path = home.join(".openclaw/config.json");
    let skill_dir = home.join(".openclaw/workspace/skills/guixu");

    let mut changed_config = false;

    if config_path.exists() {
        let raw = fs::read_to_string(&config_path).context("failed to read openclaw config")?;
        let mut root: serde_json::Value =
            serde_json::from_str(&raw).context("openclaw config is not valid JSON")?;
        let had_guixu = root
            .get("mcpServers")
            .and_then(|v| v.get("guixu"))
            .is_some();
        remove_json_mcp_server(&mut root, "guixu");
        fs::write(&config_path, serde_json::to_string_pretty(&root)? + "\n")
            .context("failed to write openclaw config")?;
        if had_guixu {
            changed_config = true;
            println!("✅ openclaw — guixu MCP entry removed");
        }
    }

    if skill_dir.exists() {
        fs::remove_dir_all(&skill_dir)
            .with_context(|| format!("failed to remove skill directory {}", skill_dir.display()))?;
        println!("✅ openclaw — skill removed at {}", skill_dir.display());
    }

    if !changed_config && !skill_dir.exists() {
        println!("openclaw — nothing to remove (guixu was not installed)");
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("guixu_mcp_test_{}_{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_openclaw_variant() {
        assert_eq!(Client::parse("openclaw"), Some(Client::OpenClaw));
        assert_eq!(Client::parse("OpenClaw"), Some(Client::OpenClaw));
        assert_eq!(Client::parse("open_claw"), Some(Client::OpenClaw));
        assert_eq!(Client::parse("open-claw"), Some(Client::OpenClaw));
    }

    #[test]
    fn parse_existing_clients_unchanged() {
        assert_eq!(Client::parse("codex"), Some(Client::Codex));
        assert_eq!(Client::parse("cursor"), Some(Client::Cursor));
        assert_eq!(Client::parse("claude-code"), Some(Client::ClaudeCode));
        assert_eq!(Client::parse("opencode"), Some(Client::OpenCode));
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert_eq!(Client::parse("vscode"), None);
    }

    #[test]
    fn display_openclaw() {
        assert_eq!(Client::OpenClaw.to_string(), "openclaw");
    }

    #[test]
    fn openclaw_config_path_ends_with_expected_suffix() {
        if let Some(path) = Client::OpenClaw.config_path() {
            assert!(path.ends_with(".openclaw/config.json"));
        }
    }

    #[test]
    fn all_clients_includes_openclaw() {
        assert!(Client::ALL.contains(&Client::OpenClaw));
    }

    #[test]
    fn openclaw_install_creates_mcp_config_entry() {
        let dir = temp_dir();
        let path = dir.join("config.json");

        // Simulate the same JSON logic as install_json_mcp_servers
        let mut root = serde_json::json!({});
        root.as_object_mut()
            .unwrap()
            .entry("mcpServers")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .unwrap()
            .insert(
                "guixu".into(),
                serde_json::json!({ "command": "/usr/local/bin/guixu", "args": ["mcp"] }),
            );
        fs::write(&path, serde_json::to_string_pretty(&root).unwrap()).unwrap();

        let content: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            content["mcpServers"]["guixu"]["command"],
            "/usr/local/bin/guixu"
        );
        assert_eq!(content["mcpServers"]["guixu"]["args"][0], "mcp");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn openclaw_install_preserves_existing_mcp_entries() {
        let dir = temp_dir();
        let path = dir.join("config.json");

        let existing = serde_json::json!({
            "mcpServers": { "other-tool": { "command": "other", "args": [] } }
        });
        fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        let mut root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        root.as_object_mut()
            .unwrap()
            .entry("mcpServers")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .unwrap()
            .insert(
                "guixu".into(),
                serde_json::json!({ "command": "guixu", "args": ["mcp"] }),
            );
        fs::write(&path, serde_json::to_string_pretty(&root).unwrap()).unwrap();

        let content: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(content["mcpServers"]["other-tool"].is_object());
        assert!(content["mcpServers"]["guixu"].is_object());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn openclaw_skill_md_contains_expected_content() {
        let dir = temp_dir();
        let skill_dir = dir.join("skills/guixu");
        fs::create_dir_all(&skill_dir).unwrap();

        let skill_md = openclaw_skill_markdown();
        fs::write(skill_dir.join("SKILL.md"), &skill_md).unwrap();

        let content = fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(content.contains("name: guixu"));
        assert!(content.contains("emoji:"));
        assert!(content.contains("intent_parse"));
        assert!(content.contains("dataset_search"));
        assert!(content.contains("dataset_evaluate"));
        assert!(content.contains("dataset_purchase"));
        assert!(content.contains("dataset_feedback"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn openclaw_uninstall_removes_guixu_entry() {
        let dir = temp_dir();
        let path = dir.join("config.json");

        let existing = serde_json::json!({
            "mcpServers": {
                "guixu": { "command": "guixu", "args": ["mcp"] },
                "other": { "command": "other", "args": [] }
            }
        });
        fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        let mut root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        root.get_mut("mcpServers")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("guixu");
        fs::write(&path, serde_json::to_string_pretty(&root).unwrap()).unwrap();

        let content: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(content["mcpServers"]["guixu"].is_null());
        assert!(content["mcpServers"]["other"].is_object());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn openclaw_uninstall_removes_skill_directory() {
        let dir = temp_dir();
        let skill_dir = dir.join("skills/guixu");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "test").unwrap();
        assert!(skill_dir.exists());

        fs::remove_dir_all(&skill_dir).unwrap();
        assert!(!skill_dir.exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn upsert_json_mcp_server_is_idempotent() {
        let mut root = serde_json::json!({
            "mcpServers": {
                "guixu": { "command": "old", "args": ["mcp"] },
                "other": { "command": "other", "args": [] }
            }
        });

        upsert_json_mcp_server(
            &mut root,
            "guixu",
            serde_json::json!({ "command": "guixu", "args": ["mcp"] }),
        )
        .unwrap();

        let servers = root["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(root["mcpServers"]["guixu"]["command"], "guixu");
        assert_eq!(root["mcpServers"]["other"]["command"], "other");
    }

    #[test]
    fn upsert_json_mcp_server_rejects_non_object_root() {
        let mut root = serde_json::json!([]);
        let err = upsert_json_mcp_server(
            &mut root,
            "guixu",
            serde_json::json!({ "command": "guixu", "args": ["mcp"] }),
        )
        .unwrap_err();

        assert!(err.to_string().contains("config is not a JSON object"));
    }

    #[test]
    fn upsert_json_mcp_server_rejects_non_object_mcp_servers() {
        let mut root = serde_json::json!({ "mcpServers": [] });
        let err = upsert_json_mcp_server(
            &mut root,
            "guixu",
            serde_json::json!({ "command": "guixu", "args": ["mcp"] }),
        )
        .unwrap_err();

        assert!(err.to_string().contains("mcpServers not object"));
    }

    #[test]
    fn remove_json_mcp_server_is_safe_when_entry_missing() {
        let mut root = serde_json::json!({
            "mcpServers": {
                "other": { "command": "other", "args": [] }
            }
        });

        remove_json_mcp_server(&mut root, "guixu");

        assert!(root["mcpServers"]["guixu"].is_null());
        assert_eq!(root["mcpServers"]["other"]["command"], "other");
    }

    #[test]
    fn openclaw_skill_markdown_includes_openclaw_metadata_and_workflow() {
        let skill_md = openclaw_skill_markdown();

        assert!(skill_md.contains("metadata:"));
        assert!(skill_md.contains("openclaw:"));
        assert!(skill_md.contains("bins:"));
        assert!(skill_md.contains("- guixu"));
        assert!(skill_md.contains("intent_parse"));
        assert!(skill_md.contains("dataset_search"));
    }
}
