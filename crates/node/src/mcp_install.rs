// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Config-driven agent adapter for `guixu mcp install/uninstall`.
//!
//! Agent definitions live in TOML (built-in defaults + optional user overrides).
//! External developers can add new agents or override existing ones without
//! touching Rust code. UDF hooks allow running external scripts for deep
//! agent-specific integration (e.g. `claude mcp add`).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ── Built-in agent definitions ───────────────────────────────────────────────

const BUILTIN_AGENTS_TOML: &str = include_str!("agents_builtin.toml");

// ── Skill / AGENTS.md content ────────────────────────────────────────────────

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

// ── Config types ─────────────────────────────────────────────────────────────

/// Top-level agents config file.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct AgentsConfig {
    #[serde(default)]
    agent: HashMap<String, AgentDef>,
}

/// A single agent definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct AgentDef {
    /// Display name.
    #[serde(default)]
    display_name: Option<String>,
    /// Config file path relative to $HOME (e.g. ".codex/config.toml").
    config_path: String,
    /// Config format: "json" or "toml".
    #[serde(default = "default_json")]
    config_format: String,
    /// JSON key path for MCP servers (e.g. "mcpServers" or "mcp").
    #[serde(default = "default_mcp_key")]
    mcp_key: String,
    /// MCP entry template. Supports `{bin}` placeholder.
    #[serde(default)]
    mcp_entry: Option<toml::Value>,
    /// Skill install directory relative to $HOME (None = no skill support).
    #[serde(default)]
    skill_dir: Option<String>,
    /// Command to detect if agent is installed (e.g. "claude").
    #[serde(default)]
    detect_cmd: Option<String>,
    /// Alternative names for parsing (e.g. ["claude", "claude-code"]).
    #[serde(default)]
    aliases: Vec<String>,
    /// Write AGENTS.md in project root (for agents that read it).
    #[serde(default)]
    write_agents_md: bool,
    /// UDF hooks: external scripts to run at lifecycle points.
    #[serde(default)]
    hooks: AgentHooks,
}

/// UDF hooks for deep agent integration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct AgentHooks {
    /// Script/command to run before install (receives bin path as $1).
    /// If it succeeds, skip the default install logic.
    #[serde(default)]
    pre_install: Option<String>,
    /// Script/command to run after install.
    #[serde(default)]
    post_install: Option<String>,
    /// Script/command to run before uninstall.
    /// If it succeeds, skip the default uninstall logic.
    #[serde(default)]
    pre_uninstall: Option<String>,
    /// Script/command to run after uninstall.
    #[serde(default)]
    post_uninstall: Option<String>,
}

fn default_json() -> String {
    "json".into()
}
fn default_mcp_key() -> String {
    "mcpServers".into()
}

// ── Registry ─────────────────────────────────────────────────────────────────

/// Load agent definitions: built-in defaults merged with optional user overrides.
fn load_agents() -> Result<HashMap<String, AgentDef>> {
    let builtin: AgentsConfig =
        toml::from_str(BUILTIN_AGENTS_TOML).context("failed to parse built-in agents config")?;
    let mut agents = builtin.agent;

    // User overrides from ~/.data-node/agents.toml
    let user_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".data-node/agents.toml");
    if user_path.exists() {
        let user_str = fs::read_to_string(&user_path)
            .with_context(|| format!("failed to read {}", user_path.display()))?;
        let user: AgentsConfig = toml::from_str(&user_str)
            .with_context(|| format!("failed to parse {}", user_path.display()))?;
        // User definitions override built-in ones by name.
        for (name, def) in user.agent {
            agents.insert(name, def);
        }
    }

    Ok(agents)
}

/// Resolve an agent name (including aliases) to its canonical name + definition.
fn resolve_agent(name: &str) -> Result<(String, AgentDef)> {
    let agents = load_agents()?;
    let normalized = name.to_lowercase().replace(['-', '_'], "");

    // Direct match
    if let Some(def) = agents.get(&normalized) {
        return Ok((normalized, def.clone()));
    }

    // Alias match
    for (canonical, def) in &agents {
        for alias in &def.aliases {
            if alias.to_lowercase().replace(['-', '_'], "") == normalized {
                return Ok((canonical.clone(), def.clone()));
            }
        }
    }

    let known: Vec<&str> = agents.keys().map(|s| s.as_str()).collect();
    anyhow::bail!(
        "unknown agent '{name}'. Known agents: {}.\n\
         Add custom agents in ~/.data-node/agents.toml",
        known.join(", ")
    )
}

// ── Helpers ──────────────────────────────────────────────────────────────────

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

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("cannot determine home directory")
}

fn expand_home(relative: &str) -> Result<PathBuf> {
    Ok(home_dir()?.join(relative))
}

/// Run a UDF hook command. Returns true if the command succeeded.
fn run_hook(hook: &str, bin: &str) -> bool {
    let result = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", hook])
            .env("GUIXU_BIN", bin)
            .status()
    } else {
        Command::new("sh")
            .args(["-c", hook])
            .env("GUIXU_BIN", bin)
            .status()
    };
    matches!(result, Ok(s) if s.success())
}

// ── JSON config helpers ──────────────────────────────────────────────────────

fn read_json(path: &Path) -> Result<serde_json::Value> {
    if path.exists() {
        let raw = fs::read_to_string(path)?;
        serde_json::from_str(&raw).with_context(|| format!("{} is not valid JSON", path.display()))
    } else {
        Ok(serde_json::json!({}))
    }
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}

fn mcp_entry_json(def: &AgentDef, bin: &str) -> serde_json::Value {
    if let Some(template) = &def.mcp_entry {
        // Convert TOML template to JSON, replacing {bin} placeholders.
        let json_str = serde_json::to_string(&template)
            .unwrap_or_default()
            .replace("{bin}", bin);
        serde_json::from_str(&json_str)
            .unwrap_or_else(|_| serde_json::json!({ "command": bin, "args": ["mcp"] }))
    } else {
        serde_json::json!({ "command": bin, "args": ["mcp"] })
    }
}

fn upsert_json_mcp(
    root: &mut serde_json::Value,
    mcp_key: &str,
    entry: serde_json::Value,
) -> Result<()> {
    root.as_object_mut()
        .context("config is not a JSON object")?
        .entry(mcp_key)
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .with_context(|| format!("{mcp_key} is not an object"))?
        .insert("guixu".into(), entry);
    Ok(())
}

fn remove_json_mcp(root: &mut serde_json::Value, mcp_key: &str) {
    if let Some(servers) = root.get_mut(mcp_key).and_then(|v| v.as_object_mut()) {
        servers.remove("guixu");
    }
}

// ── TOML config helpers ──────────────────────────────────────────────────────

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

// ── Skill generation ─────────────────────────────────────────────────────────

fn skill_markdown() -> String {
    format!(
        "---\n\
         name: guixu-data-discovery\n\
         description: >-\n\
         \x20 Autonomous data discovery, evaluation, and acquisition for AI workflows.\n\
         \x20 Searches across DeFi, academic, government, and P2P data sources.\n\
         version: 0.1.0\n\
         author: guixu-project\n\
         license: Apache-2.0\n\
         triggers:\n\
         \x20 - need training data\n\
         \x20 - find dataset\n\
         \x20 - data acquisition\n\
         \x20 - benchmark data\n\
         \x20 - labeled examples\n\
         tools:\n\
         \x20 - intent_parse\n\
         \x20 - dataset_search\n\
         \x20 - dataset_evaluate\n\
         \x20 - dataset_purchase\n\
         \x20 - dataset_feedback\n\
         \x20 - data_task_delegate\n\
         ---\n\n\
         # Guixu — Data Discovery & Market for AI Agents\n\n\
         {AGENTS_BLOCK}\n"
    )
}

// ── AGENTS.md ────────────────────────────────────────────────────────────────

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

// ── Install ──────────────────────────────────────────────────────────────────

pub fn install(name: &str) -> Result<()> {
    let (canonical, def) = resolve_agent(name)?;
    let bin = guixu_bin()?;

    // UDF pre_install hook — if it succeeds, skip default logic.
    if let Some(hook) = &def.hooks.pre_install {
        if run_hook(hook, &bin) {
            println!("✅ {canonical} configured (via pre_install hook)");
            if let Some(hook) = &def.hooks.post_install {
                run_hook(hook, &bin);
            }
            return Ok(());
        }
    }

    let config_path = expand_home(&def.config_path)?;

    match def.config_format.as_str() {
        "toml" => install_toml(&canonical, &def, &config_path, &bin)?,
        _ => install_json(&canonical, &def, &config_path, &bin)?,
    }

    // Install skill if supported.
    if let Some(skill_rel) = &def.skill_dir {
        let skill_dir = expand_home(skill_rel)?;
        fs::create_dir_all(&skill_dir)?;
        fs::write(skill_dir.join("SKILL.md"), skill_markdown())?;
        println!("   Skill:  {}", skill_dir.join("SKILL.md").display());
    }

    // Write AGENTS.md if configured.
    if def.write_agents_md {
        write_agents_md()?;
    }

    // UDF post_install hook.
    if let Some(hook) = &def.hooks.post_install {
        run_hook(hook, &bin);
    }

    Ok(())
}

fn install_json(canonical: &str, def: &AgentDef, config_path: &Path, bin: &str) -> Result<()> {
    let mut root = read_json(config_path)?;
    let entry = mcp_entry_json(def, bin);
    upsert_json_mcp(&mut root, &def.mcp_key, entry)?;
    write_json(config_path, &root)?;
    println!("✅ {canonical} configured");
    println!("   Config: {}", config_path.display());
    Ok(())
}

fn install_toml(canonical: &str, def: &AgentDef, config_path: &Path, bin: &str) -> Result<()> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut content = if config_path.exists() {
        fs::read_to_string(config_path)?
    } else {
        String::new()
    };

    // The TOML section name is derived from mcp_key (e.g. "mcp_servers.guixu").
    let section = format!("{}.guixu", def.mcp_key);
    content = remove_toml_section(&content, &section);

    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    let escaped = bin.replace('\\', "\\\\").replace('"', "\\\"");
    content.push_str(&format!(
        "\n[{section}]\ncommand = \"{escaped}\"\nargs = [\"mcp\", \"--mode\", \"codex\"]\n"
    ));

    fs::write(config_path, &content)?;
    println!("✅ {canonical} configured");
    println!("   Config: {}", config_path.display());
    Ok(())
}

// ── Uninstall ────────────────────────────────────────────────────────────────

pub fn uninstall(name: &str) -> Result<()> {
    let (canonical, def) = resolve_agent(name)?;
    let bin = guixu_bin().unwrap_or_default();

    // UDF pre_uninstall hook.
    if let Some(hook) = &def.hooks.pre_uninstall {
        if run_hook(hook, &bin) {
            println!("✅ {canonical} — guixu MCP removed (via pre_uninstall hook)");
            if let Some(hook) = &def.hooks.post_uninstall {
                run_hook(hook, &bin);
            }
            return Ok(());
        }
    }

    let config_path = expand_home(&def.config_path)?;

    match def.config_format.as_str() {
        "toml" => uninstall_toml(&canonical, &def, &config_path)?,
        _ => uninstall_json(&canonical, &def, &config_path)?,
    }

    // Remove skill directory if it exists.
    if let Some(skill_rel) = &def.skill_dir {
        let skill_dir = expand_home(skill_rel)?;
        if skill_dir.exists() {
            fs::remove_dir_all(&skill_dir)?;
            println!("✅ {canonical} — skill removed at {}", skill_dir.display());
        }
    }

    // UDF post_uninstall hook.
    if let Some(hook) = &def.hooks.post_uninstall {
        run_hook(hook, &bin);
    }

    Ok(())
}

fn uninstall_json(canonical: &str, def: &AgentDef, config_path: &Path) -> Result<()> {
    if !config_path.exists() {
        println!(
            "Nothing to remove — {} does not exist.",
            config_path.display()
        );
        return Ok(());
    }
    let mut root = read_json(config_path)?;
    remove_json_mcp(&mut root, &def.mcp_key);
    write_json(config_path, &root)?;
    println!("✅ {canonical} — guixu MCP removed");
    Ok(())
}

fn uninstall_toml(canonical: &str, _def: &AgentDef, config_path: &Path) -> Result<()> {
    if !config_path.exists() {
        println!("Nothing to remove.");
        return Ok(());
    }
    let content = fs::read_to_string(config_path)?;
    fs::write(
        config_path,
        remove_toml_section(&content, "mcp_servers.guixu"),
    )?;
    println!("✅ {canonical} — guixu MCP removed");
    Ok(())
}

// ── List ─────────────────────────────────────────────────────────────────────

pub fn list_detected() {
    let agents = match load_agents() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Failed to load agent config: {e}");
            return;
        }
    };

    println!("Supported agents:");
    let mut names: Vec<_> = agents.keys().collect();
    names.sort();
    for name in names {
        let def = &agents[name];
        let detected = is_detected(def);
        let tag = if detected { "detected" } else { "—" };
        let display = def.display_name.as_deref().unwrap_or(name);
        println!("  {display:<14} {tag}");
    }
    println!();
    println!("Add custom agents in ~/.data-node/agents.toml");
}

fn is_detected(def: &AgentDef) -> bool {
    if let Some(cmd) = &def.detect_cmd {
        return which(cmd).is_some();
    }
    expand_home(&def.config_path)
        .ok()
        .and_then(|p| p.parent().map(|d| d.exists()))
        .unwrap_or(false)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_agents_parse() {
        let agents = load_agents().unwrap();
        assert!(agents.contains_key("codex"));
        assert!(agents.contains_key("cursor"));
        assert!(agents.contains_key("claudecode"));
        assert!(agents.contains_key("opencode"));
        assert!(agents.contains_key("openclaw"));
    }

    #[test]
    fn resolve_by_alias() {
        let (name, _) = resolve_agent("claude-code").unwrap();
        assert_eq!(name, "claudecode");

        let (name, _) = resolve_agent("claude").unwrap();
        assert_eq!(name, "claudecode");
    }

    #[test]
    fn resolve_unknown_fails() {
        assert!(resolve_agent("nonexistent").is_err());
    }

    #[test]
    fn upsert_json_mcp_is_idempotent() {
        let mut root = serde_json::json!({
            "mcpServers": {
                "guixu": { "command": "old", "args": ["mcp"] },
                "other": { "command": "other", "args": [] }
            }
        });

        upsert_json_mcp(
            &mut root,
            "mcpServers",
            serde_json::json!({ "command": "guixu", "args": ["mcp"] }),
        )
        .unwrap();

        let servers = root["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(root["mcpServers"]["guixu"]["command"], "guixu");
    }

    #[test]
    fn remove_json_mcp_safe_when_missing() {
        let mut root = serde_json::json!({
            "mcpServers": { "other": { "command": "other" } }
        });
        remove_json_mcp(&mut root, "mcpServers");
        assert!(root["mcpServers"]["guixu"].is_null());
        assert!(root["mcpServers"]["other"].is_object());
    }

    #[test]
    fn skill_markdown_contains_expected_fields() {
        let md = skill_markdown();
        assert!(md.contains("name: guixu-data-discovery"));
        assert!(md.contains("intent_parse"));
        assert!(md.contains("dataset_search"));
        assert!(md.contains("data_task_delegate"));
    }

    #[test]
    fn mcp_entry_default() {
        let def = AgentDef {
            display_name: None,
            config_path: ".test/config.json".into(),
            config_format: "json".into(),
            mcp_key: "mcpServers".into(),
            mcp_entry: None,
            skill_dir: None,
            detect_cmd: None,
            aliases: vec![],
            write_agents_md: false,
            hooks: AgentHooks::default(),
        };
        let entry = mcp_entry_json(&def, "/usr/bin/guixu");
        assert_eq!(entry["command"], "/usr/bin/guixu");
        assert_eq!(entry["args"][0], "mcp");
    }

    #[test]
    fn mcp_entry_with_template() {
        let template: toml::Value = toml::from_str(
            r#"type = "local"
command = ["{bin}", "mcp"]
enabled = true"#,
        )
        .unwrap();

        let def = AgentDef {
            display_name: None,
            config_path: ".test/config.json".into(),
            config_format: "json".into(),
            mcp_key: "mcp".into(),
            mcp_entry: Some(template),
            skill_dir: None,
            detect_cmd: None,
            aliases: vec![],
            write_agents_md: false,
            hooks: AgentHooks::default(),
        };
        let entry = mcp_entry_json(&def, "/usr/bin/guixu");
        assert_eq!(entry["type"], "local");
        assert_eq!(entry["command"][0], "/usr/bin/guixu");
        assert_eq!(entry["enabled"], true);
    }
}
