// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_search::adapters::open_data_skill::load_open_data_skills;

#[test]
fn builtin_skills_are_loadable() {
    let skills = load_open_data_skills().expect("builtin skills should load");
    assert!(
        !skills.is_empty(),
        "expected at least one builtin open data skill"
    );
}

#[test]
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
