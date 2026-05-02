// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Generic CLI adapter for external AI agents.

use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tracing::{debug, info};

use crate::config::{ConnectionConfig, ExternalAgentConfig, ResponseParser};
use crate::error::{ExternalAgentError, Result};
use crate::traits::{AgentCapability, ExternalAgent};
use crate::types::{AgentHealth, AgentResponse, AgentTask};

/// Generic CLI adapter for controlling external AI agents via command line.
///
/// This adapter can work with any AI agent that provides a command-line interface.
/// Configuration drives behavior: args_template, response_parser, etc.
pub struct CliAdapter {
    config: ExternalAgentConfig,
    executable: PathBuf,
    args_template: Option<Vec<String>>,
    working_dir: Option<PathBuf>,
    env_vars: HashMap<String, String>,
    shell: Option<String>,
    capture_stderr: bool,
    response_parser: ResponseParser,
}

impl CliAdapter {
    /// Create a new CLI adapter from configuration.
    pub fn new(config: &ExternalAgentConfig) -> Result<Self> {
        let cli_config = match &config.connection {
            ConnectionConfig::Cli(cli) => cli.clone(),
            _ => {
                return Err(ExternalAgentError::Config(
                    "CLI adapter requires CLI connection configuration".to_string(),
                ))
            }
        };

        // Verify executable exists
        if !cli_config.executable.exists() {
            return Err(ExternalAgentError::Config(format!(
                "Executable not found: {}",
                cli_config.executable.display()
            )));
        }

        let response_parser = cli_config
            .response_parser
            .clone()
            .unwrap_or(ResponseParser::Text);

        Ok(Self {
            config: config.clone(),
            executable: cli_config.executable,
            args_template: cli_config.args_template,
            working_dir: cli_config.working_dir,
            env_vars: cli_config.env_vars,
            shell: cli_config.shell,
            capture_stderr: cli_config.capture_stderr,
            response_parser,
        })
    }

    /// Execute a command and capture output.
    async fn execute_command(
        &self,
        args: &[&str],
        timeout_secs: u64,
    ) -> Result<(String, String, i32)> {
        let mut command = if let Some(ref shell) = self.shell {
            let mut cmd = Command::new(shell);
            cmd.arg("-c");
            let full_command = format!("{} {}", self.executable.display(), args.join(" "));
            cmd.arg(full_command);
            cmd
        } else {
            let mut cmd = Command::new(&self.executable);
            cmd.args(args);
            cmd
        };

        // Set working directory
        if let Some(ref working_dir) = self.working_dir {
            command.current_dir(working_dir);
        }

        // Set environment variables
        for (key, value) in &self.env_vars {
            command.env(key, value);
        }

        // Configure output capture
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        debug!("Executing command: {:?}", command);

        let start = Instant::now();
        let output = command.output().await.map_err(ExternalAgentError::Io)?;

        let duration = start.elapsed();

        // Check for timeout
        if duration > Duration::from_secs(timeout_secs) {
            return Err(ExternalAgentError::Timeout(timeout_secs));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = if self.capture_stderr {
            String::from_utf8_lossy(&output.stderr).to_string()
        } else {
            String::new()
        };

        let exit_code = output.status.code().unwrap_or(-1);

        Ok((stdout, stderr, exit_code))
    }

    /// Build command arguments from task using template.
    fn build_args_from_task(&self, task: &AgentTask) -> Vec<String> {
        let mut args = Vec::new();

        // Use template if available
        if let Some(ref template) = self.args_template {
            let prompt = if !task.description.is_empty() {
                &task.description
            } else {
                task.parameters
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
            };

            for arg in template {
                if arg == "{prompt}" {
                    args.push(prompt.to_string());
                } else {
                    args.push(arg.clone());
                }
            }

            // Append extra flags from task parameters
            if let Some(flags) = task
                .parameters
                .get("extra_flags")
                .and_then(|v| v.as_array())
            {
                for flag in flags {
                    if let Some(flag_str) = flag.as_str() {
                        args.push(flag_str.to_string());
                    }
                }
            }

            return args;
        }

        // Fallback: manual arg building
        if let Some(command) = task.parameters.get("command").and_then(|v| v.as_str()) {
            args.push(command.to_string());
        }

        if let Some(subcommand) = task.parameters.get("subcommand").and_then(|v| v.as_str()) {
            args.push(subcommand.to_string());
        }

        if let Some(flags) = task.parameters.get("flags").and_then(|v| v.as_array()) {
            for flag in flags {
                if let Some(flag_str) = flag.as_str() {
                    args.push(flag_str.to_string());
                }
            }
        }

        if let Some(message) = task.parameters.get("message").and_then(|v| v.as_str()) {
            args.push(message.to_string());
        } else if !task.description.is_empty() {
            args.push(task.description.clone());
        }

        args
    }

    /// Parse response based on response_parser config.
    fn parse_response(&self, stdout: &str, stderr: &str, exit_code: i32) -> (bool, String) {
        match &self.response_parser {
            ResponseParser::Text => {
                if exit_code == 0 {
                    let content = if !stdout.is_empty() {
                        stdout.to_string()
                    } else if !stderr.is_empty() {
                        stderr.to_string()
                    } else {
                        "Command executed successfully".to_string()
                    };
                    (true, content)
                } else {
                    let error = if !stderr.is_empty() {
                        stderr.to_string()
                    } else if !stdout.is_empty() {
                        stdout.to_string()
                    } else {
                        format!("Command failed with exit code {}", exit_code)
                    };
                    (false, error)
                }
            }

            ResponseParser::JsonStream => {
                let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();

                let mut text_parts = Vec::new();
                let mut tool_outputs = Vec::new();
                let mut errors = Vec::new();

                for line in &lines {
                    if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
                        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");

                        match event_type {
                            "text" => {
                                if let Some(text) = event
                                    .get("part")
                                    .and_then(|p| p.get("text"))
                                    .and_then(|t| t.as_str())
                                {
                                    let trimmed = text.trim();
                                    if !trimmed.is_empty() {
                                        text_parts.push(trimmed.to_string());
                                    }
                                }
                            }
                            "tool_use" => {
                                // Extract tool output
                                if let Some(output) = event
                                    .get("part")
                                    .and_then(|p| p.get("state"))
                                    .and_then(|s| s.get("output"))
                                    .and_then(|o| o.as_str())
                                {
                                    let trimmed = output.trim();
                                    if !trimmed.is_empty() {
                                        tool_outputs.push(trimmed.to_string());
                                    }
                                }
                            }
                            "tool_result" => {
                                // Extract tool result output
                                if let Some(output) = event
                                    .get("part")
                                    .and_then(|p| p.get("output"))
                                    .and_then(|o| o.as_str())
                                {
                                    let trimmed = output.trim();
                                    if !trimmed.is_empty() {
                                        tool_outputs.push(trimmed.to_string());
                                    }
                                }
                            }
                            "step_finish" => {
                                // Check for errors in step finish
                                if let Some(reason) = event
                                    .get("part")
                                    .and_then(|p| p.get("reason"))
                                    .and_then(|r| r.as_str())
                                {
                                    if reason == "error" {
                                        if let Some(err) = event
                                            .get("part")
                                            .and_then(|p| p.get("error"))
                                            .and_then(|e| e.as_str())
                                        {
                                            errors.push(err.to_string());
                                        }
                                    }
                                }
                            }
                            "error" => {
                                if let Some(err) = event
                                    .get("part")
                                    .and_then(|p| p.get("message"))
                                    .and_then(|m| m.as_str())
                                {
                                    errors.push(err.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }

                if !errors.is_empty() {
                    return (false, errors.join("\n"));
                }

                // Prioritize tool outputs (actual execution results)
                if !tool_outputs.is_empty() {
                    (true, tool_outputs.join("\n"))
                } else if !text_parts.is_empty() {
                    (true, text_parts.join("\n"))
                } else if exit_code != 0 {
                    (
                        false,
                        format!("Command failed with exit code {}", exit_code),
                    )
                } else {
                    (true, "Command executed successfully".to_string())
                }
            }

            ResponseParser::Json => {
                if exit_code == 0 && !stdout.is_empty() {
                    match serde_json::from_str::<serde_json::Value>(stdout) {
                        Ok(_) => (true, stdout.to_string()),
                        Err(_) => (true, stdout.to_string()),
                    }
                } else if exit_code != 0 {
                    let error = if !stderr.is_empty() {
                        stderr.to_string()
                    } else {
                        format!("Command failed with exit code {}", exit_code)
                    };
                    (false, error)
                } else {
                    (true, stdout.to_string())
                }
            }

            ResponseParser::ExitCode => {
                if exit_code == 0 {
                    let content = if !stdout.is_empty() {
                        stdout.to_string()
                    } else {
                        "Command executed successfully".to_string()
                    };
                    (true, content)
                } else {
                    let error = if !stderr.is_empty() {
                        stderr.to_string()
                    } else {
                        format!("Command failed with exit code {}", exit_code)
                    };
                    (false, error)
                }
            }
        }
    }
}

#[async_trait]
impl ExternalAgent for CliAdapter {
    fn agent_id(&self) -> &str {
        &self.config.id
    }

    fn agent_type(&self) -> &str {
        &self.config.agent_type
    }

    async fn health_check(&self) -> Result<AgentHealth> {
        let mut metadata = HashMap::new();

        // Try to execute a simple command to check if agent is available
        let args = ["--version"];
        let (stdout, stderr, exit_code) = self.execute_command(&args, 10).await?;

        let is_reachable = exit_code == 0;

        if is_reachable {
            let version_output = if !stdout.is_empty() { &stdout } else { &stderr };
            metadata.insert("version_output".to_string(), json!(version_output.trim()));
        }

        Ok(AgentHealth {
            agent_id: self.config.id.clone(),
            is_reachable,
            version: None,
            uptime_secs: None,
            metadata,
        })
    }

    async fn execute_task(&self, task: AgentTask) -> Result<AgentResponse> {
        info!("Executing task {} on CLI agent {}", task.id, self.config.id);

        let timeout = task
            .timeout_secs
            .unwrap_or(self.config.default_timeout_secs);

        let args = self.build_args_from_task(&task);
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let start = Instant::now();
        match self.execute_command(&args_refs, timeout).await {
            Ok((stdout, stderr, exit_code)) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let (success, content) = self.parse_response(&stdout, &stderr, exit_code);

                let mut response = if success {
                    AgentResponse::success(&task.id, content)
                } else {
                    AgentResponse::failed(&task.id, content)
                };
                response.duration_ms = Some(duration_ms);
                Ok(response)
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let mut response = AgentResponse::failed(&task.id, e.to_string());
                response.duration_ms = Some(duration_ms);
                Ok(response)
            }
        }
    }

    fn capabilities(&self) -> Vec<AgentCapability> {
        vec![
            AgentCapability::TextPrompt,
            AgentCapability::ShellCommand,
            AgentCapability::TaskCancellation,
        ]
    }
}
