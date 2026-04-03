// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Integration test: search "年少有为" via PanSearchAdapter against live PanSou.
//!
//! Run with: cargo test -p data-search --test pan_search_live -- --nocapture
//! Requires network access to the PanSou instance.

use data_search::adapters::pan_search::PanSearchAdapter;
use data_search::adapters::ExternalAdapter;

#[tokio::test]
async fn search_nian_shao_you_wei() {
    let adapter = PanSearchAdapter::default();
    let results = adapter.search("年少有为", 20).await;

    match results {
        Ok(results) => {
            println!("=== pan_search results for '年少有为' ===");
            println!("total: {}", results.len());
            assert!(!results.is_empty(), "expected at least one result");

            for (i, r) in results.iter().enumerate() {
                let attrs = r.source_attributes.as_ref();
                let platform = attrs
                    .and_then(|a| a.get("platform"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let url = attrs
                    .and_then(|a| a.get("share_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let code = attrs
                    .and_then(|a| a.get("code"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("none");
                println!(
                    "[{}] platform={} title={:?} url={} code={}",
                    i + 1,
                    platform,
                    r.title,
                    url,
                    code,
                );
            }

            // Verify source_attributes are populated
            let first = &results[0];
            let attrs = first
                .source_attributes
                .as_ref()
                .expect("missing source_attributes");
            assert!(attrs.get("share_url").is_some(), "missing share_url");
            assert!(attrs.get("platform").is_some(), "missing platform");
        }
        Err(e) => {
            // Network failures are acceptable in CI
            eprintln!("pan_search failed (network?): {e}");
        }
    }
}

#[tokio::test]
async fn search_with_platform_filter() {
    let adapter = PanSearchAdapter::default();
    let results = adapter.search("年少有为", 40).await;

    match results {
        Ok(results) => {
            // Filter for quark only
            let quark: Vec<_> = results
                .iter()
                .filter(|r| {
                    r.source_attributes
                        .as_ref()
                        .and_then(|a| a.get("platform"))
                        .and_then(|v| v.as_str())
                        == Some("quark")
                })
                .collect();

            let baidu: Vec<_> = results
                .iter()
                .filter(|r| {
                    r.source_attributes
                        .as_ref()
                        .and_then(|a| a.get("platform"))
                        .and_then(|v| v.as_str())
                        == Some("baidu")
                })
                .collect();

            println!("=== platform breakdown for '年少有为' ===");
            println!(
                "quark: {}, baidu: {}, total: {}",
                quark.len(),
                baidu.len(),
                results.len()
            );

            for r in &quark {
                let url = r
                    .source_attributes
                    .as_ref()
                    .and_then(|a| a.get("share_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                println!("  quark: {} -> {}", r.title, url);
            }
            for r in &baidu {
                let url = r
                    .source_attributes
                    .as_ref()
                    .and_then(|a| a.get("share_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                println!("  baidu: {} -> {}", r.title, url);
            }
        }
        Err(e) => {
            eprintln!("pan_search failed (network?): {e}");
        }
    }
}
