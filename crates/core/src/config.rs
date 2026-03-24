use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::AccessMode;

/// Top-level node configuration, persisted at ~/.data-node/config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub data_dir: PathBuf,
    pub access_default: AccessMode,
    pub price_default: f64,
    pub listen_port: u16,
    pub bootstrap_peers: Vec<String>,
    pub node_mode: NodeMode,
    /// Privacy protection level: "off", "standard", "strict".
    #[serde(default)]
    pub privacy_level: PrivacyLevel,
    /// Differential privacy epsilon (lower = more private).
    #[serde(default = "default_epsilon")]
    pub privacy_epsilon: f64,
    /// Disable mDNS peer discovery (prevents local network IP leak).
    #[serde(default = "default_true")]
    pub disable_mdns: bool,
    /// Use ephemeral DIDs per dataset (prevents cross-dataset correlation).
    #[serde(default)]
    pub ephemeral_dids: bool,
}

fn default_epsilon() -> f64 {
    1.0
}
fn default_true() -> bool {
    true
}

/// Privacy protection level for metadata publication.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrivacyLevel {
    Off,
    #[default]
    Standard,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeMode {
    Full,
    Light,
}

impl NodeConfig {
    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".data-node")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn identity_path() -> PathBuf {
        Self::config_dir().join("identity.key")
    }

    pub fn db_path() -> PathBuf {
        Self::config_dir().join("db")
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            data_dir: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("shared-datasets"),
            access_default: AccessMode::Open,
            price_default: 0.0,
            listen_port: 9076,
            bootstrap_peers: vec![],
            node_mode: NodeMode::Full,
            privacy_level: PrivacyLevel::Standard,
            privacy_epsilon: 1.0,
            disable_mdns: true,
            ephemeral_dids: false,
        }
    }
}
