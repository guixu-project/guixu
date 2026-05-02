// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Agent registry for auto-discovering external agents.

use std::collections::HashMap;
use std::path::Path;

use tracing::{debug, info, warn};

use crate::config::ExternalAgentConfig;
use crate::error::{ExternalAgentError, Result};
use crate::traits::{AgentFactory, ExternalAgent};

/// Registry for discovering and managing external agents.
pub struct AgentRegistry {
    agents: HashMap<String, Box<dyn ExternalAgent>>,
}

impl AgentRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Scan a directory for agent configuration files (*.json).
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let mut registry = Self::new();

        if !dir.exists() {
            debug!("Agents directory not found: {}", dir.display());
            return Ok(registry);
        }

        let entries = std::fs::read_dir(dir)
            .map_err(|e| ExternalAgentError::Config(format!("Failed to read dir: {}", e)))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Only load .json files
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match Self::load_agent_config(&path) {
                    Ok(config) => match AgentFactory::create(&config) {
                        Ok(agent) => {
                            info!("Loaded agent: {} ({})", config.id, config.agent_type);
                            registry.agents.insert(config.id.clone(), agent);
                        }
                        Err(e) => {
                            warn!("Failed to create agent from {}: {}", path.display(), e);
                        }
                    },
                    Err(e) => {
                        warn!("Failed to load config {}: {}", path.display(), e);
                    }
                }
            }
        }

        info!(
            "Loaded {} agents from {}",
            registry.agents.len(),
            dir.display()
        );
        Ok(registry)
    }

    /// Load a single agent config from file.
    fn load_agent_config(path: &Path) -> Result<ExternalAgentConfig> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ExternalAgentError::Config(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let config: ExternalAgentConfig = serde_json::from_str(&content).map_err(|e| {
            ExternalAgentError::Config(format!("Invalid JSON in {}: {}", path.display(), e))
        })?;

        Ok(config)
    }

    /// Get an agent by ID.
    pub fn get(&self, id: &str) -> Option<&dyn ExternalAgent> {
        self.agents.get(id).map(|a| a.as_ref())
    }

    /// Get all agent IDs.
    pub fn list_ids(&self) -> Vec<&str> {
        self.agents.keys().map(|s| s.as_str()).collect()
    }

    /// Check if registry has an agent.
    pub fn has(&self, id: &str) -> bool {
        self.agents.contains_key(id)
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
