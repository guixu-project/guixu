// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Tests for the external agents module.

use data_external_agents::config::{
    CliConnection, ConnectionConfig, ExternalAgentConfig, HttpConnection, ResponseParser,
};
use data_external_agents::registry::AgentRegistry;
use data_external_agents::traits::{AgentFactory, ExternalAgent};
use data_external_agents::types::AgentTask;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn test_http_config_creation() {
    let config = ExternalAgentConfig::openclaw("test-agent", "http://localhost:18789");
    assert_eq!(config.id, "test-agent");
    assert_eq!(config.agent_type, "openclaw");

    match &config.connection {
        ConnectionConfig::Http(http) => {
            assert_eq!(http.base_url, "http://localhost:18789");
        }
        _ => panic!("Expected HTTP connection"),
    }
}

#[test]
fn test_cli_config_creation() {
    let config = ExternalAgentConfig::hermes("test-hermes", "/usr/local/bin/hermes");
    assert_eq!(config.id, "test-hermes");
    assert_eq!(config.agent_type, "hermes");

    match &config.connection {
        ConnectionConfig::Cli(cli) => {
            assert_eq!(cli.executable, PathBuf::from("/usr/local/bin/hermes"));
        }
        _ => panic!("Expected CLI connection"),
    }
}

#[test]
fn test_cli_config_with_template() {
    let temp_dir = tempfile::tempdir().unwrap();
    let executable = temp_dir.path().join("test-agent");
    std::fs::write(&executable, "#!/bin/sh\necho 'test'").unwrap();

    let config = ExternalAgentConfig {
        id: "test-template".to_string(),
        name: "Test Agent".to_string(),
        agent_type: "custom".to_string(),
        connection: ConnectionConfig::Cli(CliConnection {
            executable: executable,
            args_template: Some(vec![
                "run".to_string(),
                "--format".to_string(),
                "json".to_string(),
                "{prompt}".to_string(),
            ]),
            working_dir: None,
            env_vars: HashMap::new(),
            shell: None,
            capture_stderr: false,
            response_parser: Some(ResponseParser::JsonStream),
        }),
        default_timeout_secs: 30,
        max_retries: 3,
        auth: None,
        extra: HashMap::new(),
    };

    match &config.connection {
        ConnectionConfig::Cli(cli) => {
            assert!(cli.args_template.is_some());
            assert!(matches!(
                cli.response_parser,
                Some(ResponseParser::JsonStream)
            ));
        }
        _ => panic!("Expected CLI connection"),
    }
}

#[test]
fn test_task_creation() {
    let task = AgentTask::new("Test task")
        .with_timeout(30)
        .with_priority(5)
        .with_parameter("key", serde_json::json!("value"));

    assert_eq!(task.description, "Test task");
    assert_eq!(task.timeout_secs, Some(30));
    assert_eq!(task.priority, 5);
    assert_eq!(
        task.parameters.get("key").unwrap(),
        &serde_json::json!("value")
    );
}

#[test]
fn test_agent_factory_http() {
    let config = ExternalAgentConfig::openclaw("test", "http://localhost:18789");
    let agent = AgentFactory::create(&config);
    assert!(agent.is_ok());

    let agent = agent.unwrap();
    assert_eq!(agent.agent_id(), "test");
    assert_eq!(agent.agent_type(), "openclaw");
}

#[test]
fn test_agent_factory_cli() {
    let temp_dir = tempfile::tempdir().unwrap();
    let executable = temp_dir.path().join("test-agent");
    std::fs::write(&executable, "#!/bin/sh\necho 'test'").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let config = ExternalAgentConfig::hermes("test-hermes", executable.to_str().unwrap());
    let agent = AgentFactory::create(&config);
    assert!(agent.is_ok());

    let agent = agent.unwrap();
    assert_eq!(agent.agent_id(), "test-hermes");
    assert_eq!(agent.agent_type(), "hermes");
}

#[test]
fn test_agent_capabilities() {
    let config = ExternalAgentConfig::openclaw("test", "http://localhost:18789");
    let agent = AgentFactory::create(&config).unwrap();

    let capabilities = agent.capabilities();
    assert!(!capabilities.is_empty());
    assert!(capabilities.contains(&data_external_agents::traits::AgentCapability::TextPrompt));
}

#[test]
fn test_agent_registry_load() {
    // agents.d contains: opencode.json, hermes.json, openclaw.json
    // hermes.json executable doesn't exist, so only opencode + openclaw load
    let registry = AgentRegistry::load_from_dir(&PathBuf::from("../external-agents/agents.d"));
    assert!(registry.is_ok());

    let registry = registry.unwrap();
    assert!(registry.has("opencode-default"));
    assert!(registry.has("openclaw-api"));
    // hermes-local has non-existent executable, so it fails gracefully
    assert!(!registry.has("hermes-local"));
    assert_eq!(registry.list_ids().len(), 2);
}
