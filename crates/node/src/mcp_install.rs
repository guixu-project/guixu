use std::fmt;
use std::fs;
use std::path::PathBuf;

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
    Claude,
    Cursor,
    Windsurf,
    Kiro,
    Codex,
}

impl Client {
    pub const ALL: &[Client] = &[
        Client::Claude,
        Client::Cursor,
        Client::Windsurf,
        Client::Kiro,
        Client::Codex,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "cursor" => Some(Self::Cursor),
            "windsurf" => Some(Self::Windsurf),
            "kiro" => Some(Self::Kiro),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    fn config_path(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        Some(match self {
            Self::Claude => {
                if cfg!(target_os = "macos") {
                    home.join("Library/Application Support/Claude/claude_desktop_config.json")
                } else {
                    home.join(".config/Claude/claude_desktop_config.json")
                }
            }
            Self::Cursor => home.join(".cursor/mcp.json"),
            Self::Windsurf => home.join(".codeium/windsurf/mcp_config.json"),
            Self::Kiro => home.join(".kiro/settings/mcp.json"),
            Self::Codex => home.join(".codex/config.toml"),
        })
    }

    pub fn is_detected(&self) -> bool {
        self.config_path()
            .and_then(|p| p.parent().map(|d| d.exists()))
            .unwrap_or(false)
    }
}

impl fmt::Display for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Claude => write!(f, "claude"),
            Self::Cursor => write!(f, "cursor"),
            Self::Windsurf => write!(f, "windsurf"),
            Self::Kiro => write!(f, "kiro"),
            Self::Codex => write!(f, "codex"),
        }
    }
}

fn guixu_bin() -> Result<String> {
    std::env::current_exe()?
        .to_str()
        .map(String::from)
        .context("non-UTF-8 executable path")
}

// ── Install ──────────────────────────────────────────────────────────────────

pub fn install(client: Client) -> Result<()> {
    let bin = guixu_bin()?;
    match client {
        Client::Codex => install_codex(&bin),
        _ => install_json_client(client, &bin),
    }
}

fn install_json_client(client: Client, bin: &str) -> Result<()> {
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

    // Remove old [mcp_servers.guixu] section
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
        _ => uninstall_json_client(client),
    }
}

fn uninstall_json_client(client: Client) -> Result<()> {
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

// ── List ─────────────────────────────────────────────────────────────────────

pub fn list_detected() {
    println!("Supported clients:");
    for c in Client::ALL {
        let tag = if c.is_detected() { "detected" } else { "—" };
        println!("  {c:<12} {tag}");
    }
}
