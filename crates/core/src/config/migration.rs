// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Configuration migration for backward compatibility.
//!
//! GIP005 requires that old config files (without `features` section) should infer defaults based on `NodeMode`.

use super::{NodeConfig, NodeMode};

/// Migrate a raw config (parsed or partial) to include the `features` section.
/// If `features.blockchain_payment` is explicitly set (non-default-true), skip migration
/// to avoid overwriting intentional settings.
pub fn apply_config_migration(config: &mut NodeConfig) {
    // If blockchain_payment is explicitly false, the config was written with a newer
    // version that explicitly set features — don't overwrite.
    if !config.features.blockchain_payment {
        return;
    }

    // Apply NodeMode-based feature defaults for old configs that still have the
    // default FeaturesConfig (all features == true, meaning it was never written out).
    match config.node_mode {
        NodeMode::Full => {
            // Full mode: all features enabled (default), only heavy adapters disabled
            // No change needed — FeaturesConfig::default() already matches.
        }
        NodeMode::Light => {
            // Light mode: disable all heavy subsystems.
            // Override adapter_blacklist in features to match GIP005 design.
            config.features.adapter_blacklist = vec![
                "google_dataset_search".into(),
                "ipfs".into(),
                "huggingface".into(),
                "bittorrent".into(),
            ];
            // Also sync legacy disabled_adapters for any code that still reads it directly.
            if config.disabled_adapters.is_empty() {
                config.disabled_adapters = vec![
                    "google_dataset_search".into(),
                    "ipfs".into(),
                    "huggingface".into(),
                    "bittorrent".into(),
                ];
            }
        }
    }
}

/// Build the effective adapter blacklist from both the legacy `disabled_adapters`
/// field and the new `features.adapter_blacklist`.
pub fn resolve_adapter_blacklist(config: &NodeConfig) -> Vec<String> {
    let mut blacklist = config.disabled_adapters.clone();
    for adapter in &config.features.adapter_blacklist {
        if !blacklist.contains(adapter) {
            blacklist.push(adapter.clone());
        }
    }
    blacklist
}
