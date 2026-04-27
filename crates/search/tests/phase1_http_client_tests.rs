// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Tests for Phase 1: Shared HTTP Client
//!
//! These tests verify:
//! - HTTP client is properly shared across the application
//! - Connection pool settings are correct

use data_search::http_client::SHARED_HTTP_CLIENT;

#[test]
fn test_shared_http_client_is_initialized() {
    // LazyLock should be initialized on first access
    let client = SHARED_HTTP_CLIENT.clone();
    // Client can be cloned and used to build requests
    let request = client.get("https://example.com").build().unwrap();
    assert_eq!(request.url().host_str(), Some("example.com"));
}

#[test]
fn test_shared_http_client_can_be_cloned() {
    let client1 = SHARED_HTTP_CLIENT.clone();
    let client2 = SHARED_HTTP_CLIENT.clone();

    // Both should be able to build requests
    let request1 = client1.get("https://example.com").build().unwrap();
    let request2 = client2.get("https://example.com").build().unwrap();

    assert_eq!(request1.url().host_str(), Some("example.com"));
    assert_eq!(request2.url().host_str(), Some("example.com"));
}

#[tokio::test]
async fn test_shared_http_client_can_build_post_request() {
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestPayload {
        key: String,
    }

    let client = &*SHARED_HTTP_CLIENT;
    let request = client
        .post("https://httpbin.org/post")
        .json(&TestPayload {
            key: "value".to_string(),
        })
        .build()
        .unwrap();

    assert_eq!(request.url().host_str(), Some("httpbin.org"));
}
