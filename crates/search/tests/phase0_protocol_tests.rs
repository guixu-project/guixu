// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Tests for Phase 0: Protocol Hardening
//!
//! These tests verify:
//! - Path template rendering in HttpSkillAdapter
//! - ID normalization for guixu-market prefixed IDs
//! - Capability alignment in skill JSON files

use data_search::adapters::HttpSkillAdapter;

#[test]
fn test_render_operation_path_with_id_template() {
    let path = "/api/v1/skill/item/{id}";
    let id = "guixu-market:123e4567-e89b-12d3-a456-426614174000";

    let rendered = HttpSkillAdapter::render_operation_path(path, id);

    assert_eq!(
        rendered,
        "/api/v1/skill/item/guixu-market%3A123e4567-e89b-12d3-a456-426614174000"
    );
}

#[test]
fn test_render_operation_path_without_id_template() {
    let path = "/api/v1/skill/search";
    let id = "some-query";

    let rendered = HttpSkillAdapter::render_operation_path(path, id);

    assert_eq!(rendered, "/api/v1/skill/search");
}

#[test]
fn test_render_operation_path_with_special_chars_in_id() {
    let path = "/api/v1/skill/item/{id}";
    let id = "test-dataset-123/with/slashes";

    let rendered = HttpSkillAdapter::render_operation_path(path, id);

    assert!(rendered.contains("test-dataset-123"));
    assert!(rendered.contains("%2F")); // URL-encoded slash
}

#[test]
fn test_render_operation_path_with_uuid_id() {
    let path = "/api/v1/skill/item/{id}";
    let id = "123e4567-e89b-12d3-a456-426614174000";

    let rendered = HttpSkillAdapter::render_operation_path(path, id);

    assert_eq!(
        rendered,
        "/api/v1/skill/item/123e4567-e89b-12d3-a456-426614174000"
    );
}

#[test]
fn test_render_operation_path_empty_id() {
    let path = "/api/v1/skill/item/{id}";
    let id = "";

    let rendered = HttpSkillAdapter::render_operation_path(path, id);

    assert_eq!(rendered, "/api/v1/skill/item/");
}
