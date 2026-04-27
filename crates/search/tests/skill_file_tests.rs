// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Tests for Skill JSON Files
//!
//! These tests verify:
//! - Skill files are valid JSON
//! - Required fields are present
//! - Capabilities match actual implementation

use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
struct SkillFile {
    spec_version: String,
    id: String,
    name: String,
    description: String,
    source: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    routing_hints: Vec<String>,
    #[serde(default)]
    enabled: bool,
    capabilities: SkillCapabilities,
    governance: SkillGovernance,
    provider: Provider,
}

#[derive(Debug, Deserialize)]
struct SkillCapabilities {
    #[serde(default)]
    search: bool,
    #[serde(default)]
    lookup: bool,
    #[serde(default)]
    download: bool,
    #[serde(default)]
    schema_probe: bool,
    #[serde(default)]
    sample_preview: bool,
    #[serde(default)]
    license_lookup: bool,
    #[serde(default)]
    query: bool,
    #[serde(default)]
    subscribe: bool,
    #[serde(default)]
    backfill: bool,
    #[serde(default)]
    decode: bool,
    #[serde(default)]
    simulate: bool,
    #[serde(default)]
    execute: bool,
}

#[derive(Debug, Deserialize)]
struct SkillGovernance {
    #[serde(default)]
    trust_tier: String,
    #[serde(default)]
    provenance_hint: Option<String>,
    #[serde(default)]
    compliance_hint: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Provider {
    HttpSearch {
        base_url: String,
        #[serde(default)]
        operations: HttpOperations,
        #[serde(default)]
        auth: serde_json::Value,
    },
    NativeAdapter,
    SqlCatalog,
}

#[derive(Debug, Deserialize, Default)]
struct HttpOperations {
    #[serde(default)]
    search: Option<HttpOperation>,
    #[serde(default)]
    lookup: Option<HttpOperation>,
    #[serde(default)]
    download: Option<HttpOperation>,
}

#[derive(Debug, Deserialize)]
struct HttpOperation {
    path: String,
    #[serde(default)]
    method: String,
}

fn load_skill_file(path: &str) -> SkillFile {
    let content = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!("Failed to read {}: {}", path, e);
    });
    serde_json::from_str(&content).unwrap_or_else(|e| {
        panic!("Failed to parse {}: {}", path, e);
    })
}

#[test]
fn test_guixu_market_skill_file_is_valid_json() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");
    assert_eq!(skill.id, "guixu_market");
}

#[test]
fn test_guixu_market_skill_uses_v2_spec() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");
    assert_eq!(skill.spec_version, "2.0");
}

#[test]
fn test_guixu_market_skill_has_required_fields() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");

    assert!(!skill.name.is_empty());
    assert!(!skill.description.is_empty());
    assert!(!skill.source.is_empty());
}

#[test]
fn test_guixu_market_skill_has_search_capability() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");
    assert!(skill.capabilities.search);
}

#[test]
fn test_guixu_market_skill_has_lookup_capability() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");
    assert!(skill.capabilities.lookup);
}

#[test]
fn test_guixu_market_skill_download_capability_is_false() {
    // As per Phase 0, download should be false since it's not properly configured
    let skill = load_skill_file("skills/builtin/guixu_market.json");
    assert!(!skill.capabilities.download);
}

#[test]
fn test_guixu_market_skill_has_governance() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");
    assert!(!skill.governance.trust_tier.is_empty());
}

#[test]
fn test_guixu_market_skill_has_provenance_hint() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");
    assert!(skill.governance.provenance_hint.is_some());
}

#[test]
fn test_guixu_market_skill_has_compliance_hint() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");
    assert!(skill.governance.compliance_hint.is_some());
}

#[test]
fn test_guixu_market_skill_has_routing_hints() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");
    assert!(!skill.routing_hints.is_empty());
}

#[test]
fn test_guixu_market_skill_search_operation_is_configured() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");

    match skill.provider {
        Provider::HttpSearch { operations, .. } => {
            assert!(
                operations.search.is_some(),
                "search operation should be configured"
            );
            let search = operations.search.unwrap();
            assert!(!search.path.is_empty(), "search path should not be empty");
        }
        _ => panic!("guixu_market should be http_search provider"),
    }
}

#[test]
fn test_guixu_market_skill_lookup_operation_is_configured() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");

    match skill.provider {
        Provider::HttpSearch { operations, .. } => {
            assert!(
                operations.lookup.is_some(),
                "lookup operation should be configured"
            );
            let lookup = operations.lookup.unwrap();
            let id_template = "{id}";
            assert!(
                lookup.path.contains(id_template),
                "lookup path should contain {{id}} template"
            );
        }
        _ => panic!("guixu_market should be http_search provider"),
    }
}

#[test]
fn test_guixu_market_skill_has_correct_base_url() {
    let skill = load_skill_file("skills/builtin/guixu_market.json");

    match skill.provider {
        Provider::HttpSearch { base_url, .. } => {
            assert!(base_url.contains("market.guixu.io") || base_url.contains("localhost"));
        }
        _ => panic!("guixu_market should be http_search provider"),
    }
}

#[test]
fn test_all_builtin_skills_are_valid() {
    let skills_dir = std::path::Path::new("skills/builtin");
    if !skills_dir.exists() {
        return; // Skip if directory doesn't exist
    }

    for entry in fs::read_dir(skills_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let content = fs::read_to_string(&path).unwrap();
            let result: Result<serde_json::Value, _> = serde_json::from_str(&content);
            assert!(result.is_ok(), "Failed to parse: {:?}", path);
        }
    }
}
