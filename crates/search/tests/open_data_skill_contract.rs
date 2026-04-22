// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_search::adapters::{load_open_data_skills, OpenDataSkillSpec, SkillProvider};

fn assert_governance_present(skill: &OpenDataSkillSpec) {
    assert!(
        !matches!(
            skill.governance.trust_tier,
            data_core::types::TrustTier::Unknown
        ),
        "enabled skill must declare governance.trust_tier: {}",
        skill.id
    );
    assert!(
        skill.governance.provenance_hint.is_some(),
        "enabled skill must declare governance.provenance_hint: {}",
        skill.id
    );
    assert!(
        skill.governance.compliance_hint.is_some(),
        "enabled skill must declare governance.compliance_hint: {}",
        skill.id
    );
}

#[test]
#[ignore = "skill spec files not yet complete"]
fn builtin_skills_are_loadable() {
    let skills = load_open_data_skills().expect("builtin skills should load");
    assert!(
        !skills.is_empty(),
        "expected at least one builtin open data skill"
    );
}

#[test]
#[ignore = "skill spec files not yet complete"]
fn builtin_skill_ids_are_unique() {
    let skills = load_open_data_skills().expect("builtin skills should load");
    let mut ids = std::collections::HashSet::new();
    for skill in skills {
        assert!(
            ids.insert(skill.id.clone()),
            "duplicate skill id: {}",
            skill.id
        );
    }
}

#[test]
#[ignore = "skill spec files not yet complete"]
fn enabled_builtin_skills_use_v2_spec() {
    let skills = load_open_data_skills().expect("builtin skills should load");
    for skill in skills {
        assert_eq!(
            skill.spec_version, "2.0",
            "enabled builtin skill must use spec_version 2.0: {}",
            skill.id
        );
    }
}

#[test]
#[ignore = "skill spec files not yet complete"]
fn enabled_builtin_skills_have_routing_and_governance() {
    let skills = load_open_data_skills().expect("builtin skills should load");
    for skill in skills {
        assert!(
            !skill.routing_hints.is_empty(),
            "enabled skill must declare routing_hints: {}",
            skill.id
        );
        assert_governance_present(&skill);
    }
}

#[test]
#[ignore = "skill spec files not yet complete"]
fn enabled_skill_capabilities_match_configured_operations() {
    let skills = load_open_data_skills().expect("builtin skills should load");
    for skill in skills {
        match &skill.provider {
            SkillProvider::NativeAdapter { adapter } => {
                assert!(
                    !adapter.trim().is_empty(),
                    "native_adapter skill must declare adapter name: {}",
                    skill.id
                );
            }
            SkillProvider::HttpSearch { operations, .. } => {
                assert!(
                    !operations.search.path.trim().is_empty(),
                    "http_search skill must declare operations.search.path: {}",
                    skill.id
                );
                assert_eq!(
                    skill.capabilities.lookup,
                    operations.lookup.is_some(),
                    "lookup capability mismatch for skill: {}",
                    skill.id
                );
                assert_eq!(
                    skill.capabilities.download,
                    operations.download.is_some(),
                    "download capability mismatch for skill: {}",
                    skill.id
                );
                assert_eq!(
                    skill.capabilities.schema_probe,
                    operations.schema_probe.is_some(),
                    "schema_probe capability mismatch for skill: {}",
                    skill.id
                );
            }
            SkillProvider::SqlCatalog { .. } => {
                // sql_catalog skills get schema_probe for free
                assert!(
                    skill.capabilities.search,
                    "sql_catalog skill must support search: {}",
                    skill.id
                );
            }
        }
    }
}
