// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetStats;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Privacy protection level for metadata publication.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrivacyLevel {
    /// No privacy protection (current behaviour).
    Off,
    /// Standard: DP noise on stats, hash sensitive column names.
    #[default]
    Standard,
    /// Strict: all of Standard + suppress min/max entirely, hash ALL column names.
    Strict,
}

/// Configuration for differential privacy noise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    pub level: PrivacyLevel,
    /// Differential privacy epsilon (lower = more private, default 1.0).
    pub epsilon: f64,
    /// Patterns considered sensitive (regex-free substring match for simplicity).
    pub sensitive_patterns: Vec<String>,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            level: PrivacyLevel::Standard,
            epsilon: 1.0,
            sensitive_patterns: vec![
                "ssn".into(),
                "social_security".into(),
                "diagnosis".into(),
                "medical".into(),
                "salary".into(),
                "income".into(),
                "phone".into(),
                "email".into(),
                "address".into(),
                "passport".into(),
                "credit_card".into(),
                "bank_account".into(),
                "dob".into(),
                "date_of_birth".into(),
                "password".into(),
            ],
        }
    }
}

/// Sanitise metadata before publishing to the P2P network.
/// Returns a new DatasetMetadata with privacy protections applied.
pub fn sanitize_metadata(
    metadata: &DatasetMetadata,
    config: &PrivacyConfig,
) -> Result<DatasetMetadata> {
    if config.level == PrivacyLevel::Off {
        return Ok(metadata.clone());
    }

    let mut m = metadata.clone();

    // 1. Sanitise column names
    m.schema.columns = m
        .schema
        .columns
        .iter()
        .map(|col| {
            let should_hash = match config.level {
                PrivacyLevel::Strict => true,
                PrivacyLevel::Standard => is_sensitive(&col.name, &config.sensitive_patterns),
                PrivacyLevel::Off => false,
            };
            if should_hash {
                let mut c = col.clone();
                c.name = hash_column_name(&col.name);
                c.description = None; // strip description for hashed columns
                c
            } else {
                col.clone()
            }
        })
        .collect();

    // 2. Sanitise tags (remove sensitive ones)
    m.tags = m
        .tags
        .iter()
        .filter(|tag| !is_sensitive(tag, &config.sensitive_patterns))
        .cloned()
        .collect();

    // 3. Apply DP noise to stats
    if let Some(ref stats) = m.stats {
        m.stats = Some(sanitize_stats(stats, config)?);
    }

    Ok(m)
}

/// Check if a name matches any sensitive pattern (case-insensitive substring).
fn is_sensitive(name: &str, patterns: &[String]) -> bool {
    let lower = name.to_lowercase();
    patterns.iter().any(|p| lower.contains(p))
}

/// One-way hash a column name: `sha256(name)[..8]` prefixed with `h_`.
fn hash_column_name(name: &str) -> String {
    let hash = Sha256::digest(name.to_lowercase().as_bytes());
    format!("h_{}", hex::encode(&hash[..4]))
}

/// Apply ε-differential privacy (Laplace mechanism) to dataset statistics.
fn sanitize_stats(stats: &DatasetStats, config: &PrivacyConfig) -> Result<DatasetStats> {
    let eps = config.epsilon;

    let mut s = stats.clone();

    // Laplace noise: sensitivity/ε.  For rates in [0,1], sensitivity = 1.
    s.null_rate = (s.null_rate + laplace_noise(1.0 / eps)).clamp(0.0, 1.0);
    s.unique_rate = (s.unique_rate + laplace_noise(1.0 / eps)).clamp(0.0, 1.0);

    // Min/max values: suppress in Strict mode, add noise in Standard
    match config.level {
        PrivacyLevel::Strict => {
            s.min_values = serde_json::Value::Null;
            s.max_values = serde_json::Value::Null;
        }
        PrivacyLevel::Standard => {
            s.min_values = add_noise_to_json_values(&s.min_values, eps);
            s.max_values = add_noise_to_json_values(&s.max_values, eps);
        }
        PrivacyLevel::Off => {}
    }

    Ok(s)
}

/// Add Laplace noise to all numeric values in a JSON object.
fn add_noise_to_json_values(val: &serde_json::Value, epsilon: f64) -> serde_json::Value {
    match val {
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k.clone(), add_noise_to_json_values(v, epsilon));
            }
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                // Sensitivity heuristic: 1% of absolute value, minimum 1.0
                let sensitivity = (f.abs() * 0.01).max(1.0);
                let noisy = f + laplace_noise(sensitivity / epsilon);
                serde_json::json!(noisy)
            } else {
                val.clone()
            }
        }
        _ => val.clone(),
    }
}

/// Sample from Laplace(0, scale) using inverse CDF.
/// Uses deterministic-ish randomness from std; for production use a CSPRNG.
fn laplace_noise(scale: f64) -> f64 {
    // Use thread_rng for proper randomness
    let u: f64 = rand::random::<f64>() - 0.5; // Uniform(-0.5, 0.5)
    -scale * u.signum() * (1.0 - 2.0 * u.abs()).ln()
}

/// Filter tags to exclude sensitive column-derived tags before DHT indexing.
pub fn safe_tags_for_dht(tags: &[String], config: &PrivacyConfig) -> Vec<String> {
    if config.level == PrivacyLevel::Off {
        return tags.to_vec();
    }
    tags.iter()
        .filter(|t| !is_sensitive(t, &config.sensitive_patterns))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_detection() {
        let patterns = PrivacyConfig::default().sensitive_patterns;
        assert!(is_sensitive("patient_ssn", &patterns));
        assert!(is_sensitive("Email_Address", &patterns));
        assert!(is_sensitive("monthly_salary", &patterns));
        assert!(!is_sensitive("temperature", &patterns));
        assert!(!is_sensitive("timestamp", &patterns));
    }

    #[test]
    fn column_hashing_deterministic() {
        let a = hash_column_name("ssn");
        let b = hash_column_name("ssn");
        assert_eq!(a, b);
        assert!(a.starts_with("h_"));
        assert_ne!(a, hash_column_name("email"));
    }

    #[test]
    fn laplace_noise_has_zero_mean_approx() {
        let samples: Vec<f64> = (0..10_000).map(|_| laplace_noise(1.0)).collect();
        let mean: f64 = samples.iter().sum::<f64>() / samples.len() as f64;
        assert!(mean.abs() < 0.1, "mean should be near 0, got {mean}");
    }

    #[test]
    fn dp_stats_clamp_to_valid_range() {
        let stats = DatasetStats {
            null_rate: 0.01,
            unique_rate: 0.99,
            min_values: serde_json::json!({"a": 0}),
            max_values: serde_json::json!({"a": 100}),
        };
        let config = PrivacyConfig {
            epsilon: 0.1,
            ..Default::default()
        };
        let sanitized = sanitize_stats(&stats, &config).unwrap();
        assert!((0.0..=1.0).contains(&sanitized.null_rate));
        assert!((0.0..=1.0).contains(&sanitized.unique_rate));
    }

    #[test]
    fn strict_suppresses_min_max() {
        let stats = DatasetStats {
            null_rate: 0.05,
            unique_rate: 0.8,
            min_values: serde_json::json!({"salary": 30000}),
            max_values: serde_json::json!({"salary": 500000}),
        };
        let config = PrivacyConfig {
            level: PrivacyLevel::Strict,
            epsilon: 1.0,
            ..Default::default()
        };
        let sanitized = sanitize_stats(&stats, &config).unwrap();
        assert_eq!(sanitized.min_values, serde_json::Value::Null);
        assert_eq!(sanitized.max_values, serde_json::Value::Null);
    }

    #[test]
    fn safe_tags_filters_sensitive() {
        let tags = vec![
            "weather".into(),
            "ssn".into(),
            "temperature".into(),
            "email_list".into(),
        ];
        let config = PrivacyConfig::default();
        let safe = safe_tags_for_dht(&tags, &config);
        assert_eq!(safe, vec!["weather", "temperature"]);
    }
}
