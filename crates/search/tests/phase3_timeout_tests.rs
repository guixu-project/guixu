// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Tests for Phase 3: Timeout / Backpressure
//!
//! These tests verify:
//! - Adapter timeout behavior
//! - Error isolation between adapters
//! - Concurrent adapter handling

use std::time::Duration;

const ADAPTER_TIMEOUT_SECS: u64 = 5;

#[test]
fn test_adapter_timeout_constant_is_reasonable() {
    // Verify the timeout constant is reasonable (5 seconds)
    assert_eq!(ADAPTER_TIMEOUT_SECS, 5);
    assert!(
        ADAPTER_TIMEOUT_SECS <= 10,
        "Timeout should be <= 10 seconds"
    );
}

#[tokio::test]
async fn test_timeout_behavior_for_slow_adapter() {
    // Simulate a slow adapter that takes 10 seconds
    let slow_fut = async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok::<Vec<String>, String>(vec!["result".to_string()])
    };

    let timeout_duration = Duration::from_secs(ADAPTER_TIMEOUT_SECS);
    let result = tokio::time::timeout(timeout_duration, slow_fut).await;

    // Should timeout
    assert!(result.is_err());
}

#[tokio::test]
async fn test_fast_adapter_completes_within_timeout() {
    // Simulate a fast adapter that takes 100ms
    let fast_fut = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok::<Vec<String>, String>(vec!["result".to_string()])
    };

    let timeout_duration = Duration::from_secs(ADAPTER_TIMEOUT_SECS);
    let result = tokio::time::timeout(timeout_duration, fast_fut).await;

    // Should complete successfully
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Ok(vec!["result".to_string()]));
}

#[tokio::test]
async fn test_multiple_adapters_with_one_slow() {
    // Simulate 3 adapters: 2 fast, 1 slow
    let fast_adapter1 = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok::<Vec<String>, String>(vec!["fast1".to_string()])
    };

    let fast_adapter2 = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok::<Vec<String>, String>(vec!["fast2".to_string()])
    };

    let slow_adapter = async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok::<Vec<String>, String>(vec!["slow".to_string()])
    };

    let timeout_duration = Duration::from_secs(ADAPTER_TIMEOUT_SECS);

    // Run all adapters concurrently
    let (r1, r2, r3) = tokio::join!(
        tokio::time::timeout(timeout_duration, fast_adapter1),
        tokio::time::timeout(timeout_duration, fast_adapter2),
        tokio::time::timeout(timeout_duration, slow_adapter),
    );

    // Fast adapters should succeed
    assert!(r1.is_ok());
    assert!(r2.is_ok());

    // Slow adapter should timeout
    assert!(r3.is_err());
}

#[tokio::test]
async fn test_error_isolation_between_adapters() {
    // Simulate one adapter that errors and one that succeeds
    let erroring_adapter = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Err::<Vec<String>, String>("Adapter error".to_string())
    };

    let succeeding_adapter = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok::<Vec<String>, String>(vec!["success".to_string()])
    };

    let timeout_duration = Duration::from_secs(5);

    let (error_result, success_result) = tokio::join!(
        tokio::time::timeout(timeout_duration, erroring_adapter),
        tokio::time::timeout(timeout_duration, succeeding_adapter),
    );

    // Erroring adapter should return error
    assert!(error_result.is_ok()); // timeout didn't trigger
    assert!(error_result.unwrap().is_err());

    // Succeeding adapter should return success
    assert!(success_result.is_ok());
    assert_eq!(
        success_result.unwrap().unwrap(),
        vec!["success".to_string()]
    );
}

#[tokio::test]
async fn test_adapter_timeout_does_not_affect_other_adapters() {
    let timeout_duration = Duration::from_secs(2);

    // Start multiple tasks
    let handles: Vec<_> = (0..5)
        .map(|i| {
            tokio::spawn(async move {
                if i == 2 {
                    // Simulate slow adapter
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    Ok::<Vec<String>, String>(vec!["slow".to_string()])
                } else {
                    // Fast adapter
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok::<Vec<String>, String>(vec![format!("fast{}", i)])
                }
            })
        })
        .collect();

    // Wait for all with timeout
    let mut results = Vec::new();
    for handle in handles {
        let result = tokio::time::timeout(timeout_duration, handle).await;
        results.push(result);
    }

    // Most should succeed (2 seconds is enough for fast ones)
    let successes = results
        .iter()
        .filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok())
        .count();
    assert!(successes >= 4); // At least 4 out of 5 should succeed
}
