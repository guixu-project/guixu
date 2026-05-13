// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Tests for x402 guixu.market integration changes.
//! Run with: cargo test -p data-trading x402_market

use data_core::types::*;
use data_trading::router::{PaymentRouter, TransactionContext};

fn make_ctx(
    amount: f64,
    seller_endpoint: Option<&str>,
    seller_headers: Option<Vec<(String, String)>>,
) -> TransactionContext {
    TransactionContext {
        buyer: Did("did:test:buyer".into()),
        seller: Did("did:test:seller".into()),
        dataset_cid: DatasetCid("guixu-market:test-123".into()),
        amount,
        is_single_request: true,
        is_session_batch: false,
        prefer_fiat: false,
        requires_verification: amount > 1.0,
        seller_endpoint: seller_endpoint.map(String::from),
        seller_headers,
    }
}

// ============================================================================
// TransactionContext seller_headers tests
// ============================================================================

#[test]
fn test_transaction_context_with_seller_headers() {
    let headers = vec![
        ("Authorization".into(), "Bearer test-token-123".into()),
        ("X-Custom".into(), "value".into()),
    ];
    let ctx = make_ctx(
        0.005,
        Some("https://market.guixu.io/api/v1/x402/datasets/abc/download"),
        Some(headers.clone()),
    );

    assert_eq!(ctx.seller_headers.as_ref().unwrap().len(), 2);
    assert_eq!(ctx.seller_headers.as_ref().unwrap()[0].0, "Authorization");
    assert_eq!(
        ctx.seller_headers.as_ref().unwrap()[0].1,
        "Bearer test-token-123"
    );
}

#[test]
fn test_transaction_context_without_seller_headers() {
    let ctx = make_ctx(0.005, None, None);
    assert!(ctx.seller_headers.is_none());
}

fn test_wallet() -> data_trading::wallet::AgentWallet {
    // Deterministic test key (DO NOT use in production)
    data_trading::wallet::AgentWallet::from_private_key(
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    )
    .unwrap()
}

// ============================================================================
// Protocol selection tests (unchanged behavior)
// ============================================================================

#[test]
fn test_protocol_selection_x402_for_micropayment() {
    let ctx = make_ctx(0.005, None, None);
    let router = PaymentRouter::from_wallet(test_wallet(), true);
    let protocol = router.select_protocol(&ctx);
    assert_eq!(protocol, PaymentProtocol::X402);
}

#[test]
fn test_protocol_selection_escrow_for_large_verified() {
    let ctx = make_ctx(5.0, None, None);
    let router = PaymentRouter::from_wallet(test_wallet(), true);
    let protocol = router.select_protocol(&ctx);
    assert_eq!(protocol, PaymentProtocol::Erc8183);
}

#[test]
fn test_protocol_selection_mpp_for_session() {
    let ctx = TransactionContext {
        buyer: Did("did:test:buyer".into()),
        seller: Did("did:test:seller".into()),
        dataset_cid: DatasetCid("test".into()),
        amount: 0.50,
        is_single_request: false,
        is_session_batch: true,
        prefer_fiat: false,
        requires_verification: false,
        seller_endpoint: None,
        seller_headers: None,
    };
    let router = PaymentRouter::from_wallet(test_wallet(), true);
    let protocol = router.select_protocol(&ctx);
    assert_eq!(protocol, PaymentProtocol::StripeMpp);
}

// ============================================================================
// Seller endpoint resolution tests
// ============================================================================

#[test]
fn test_seller_endpoint_explicit() {
    let ctx = make_ctx(
        0.005,
        Some("https://market.guixu.io/api/v1/x402/datasets/abc/download"),
        None,
    );
    assert_eq!(
        ctx.seller_endpoint.unwrap(),
        "https://market.guixu.io/api/v1/x402/datasets/abc/download"
    );
}

#[test]
fn test_seller_endpoint_none_uses_default() {
    let ctx = make_ctx(0.005, None, None);
    assert!(ctx.seller_endpoint.is_none());
    // PaymentRouter::pay will use fallback "http://localhost:4242/paid"
}

// ============================================================================
// PaymentProtocol PartialEq (needed for pay_with_fallback)
// ============================================================================

#[test]
fn test_payment_protocol_eq() {
    assert_eq!(PaymentProtocol::X402, PaymentProtocol::X402);
    assert_ne!(PaymentProtocol::X402, PaymentProtocol::Erc8183);
    assert_ne!(PaymentProtocol::StripeMpp, PaymentProtocol::Erc8183);
}

// ============================================================================
// Skill item mapping tests (price + seller_endpoint from JSON)
// ============================================================================

#[test]
fn test_skill_item_mapping_price_parsing() {
    // Simulate what the Open Data Skill adapter does with price fields
    let item = serde_json::json!({
        "id": "guixu-market:abc-123",
        "title": "Test Dataset",
        "description": "A test",
        "download_url": "https://market.guixu.io/api/v1/x402/datasets/abc-123/download",
        "price": {
            "amount": "1.50",
            "currency": "USDC",
            "payment_protocol": "x402"
        }
    });

    // Test nested path extraction (simulating string_at_path)
    let amount_str = item.pointer("/price/amount").and_then(|v| v.as_str());
    assert_eq!(amount_str, Some("1.50"));

    let currency = item.pointer("/price/currency").and_then(|v| v.as_str());
    assert_eq!(currency, Some("USDC"));

    let download_url = item.get("download_url").and_then(|v| v.as_str());
    assert!(download_url.unwrap().contains("/x402/"));
}

#[test]
fn test_skill_item_mapping_free_dataset() {
    let item = serde_json::json!({
        "id": "guixu-market:free-123",
        "title": "Free Dataset",
        "download_url": "https://market.guixu.io/api/v1/x402/datasets/free-123/download"
    });

    // No price field → should be treated as free
    let price = item.get("price");
    assert!(price.is_none());
}

// ============================================================================
// guixu_market.json skill file validation
// ============================================================================

#[test]
fn test_guixu_market_skill_file_valid() {
    let skill_json = include_str!("../../search/skills/builtin/guixu_market.json");
    let skill: serde_json::Value =
        serde_json::from_str(skill_json).expect("Invalid JSON in guixu_market.json");

    assert_eq!(skill["id"], "guixu_market");
    assert_eq!(skill["spec_version"], "2.0");
    assert_eq!(skill["enabled"], true);

    // Capabilities
    assert_eq!(skill["capabilities"]["search"], true);
    assert_eq!(skill["capabilities"]["lookup"], true);
    assert_eq!(skill["capabilities"]["download"], true);

    // Auth config
    assert_eq!(skill["provider"]["auth"]["kind"], "header_env");
    assert_eq!(skill["provider"]["auth"]["env"], "GUIXU_MARKET_API_TOKEN");

    // Payment config
    assert_eq!(skill["provider"]["payment"]["protocol"], "x402");
    assert_eq!(skill["provider"]["payment"]["network"], "eip155:8453");

    // Item mapping includes price and seller_endpoint
    let mapping = &skill["provider"]["item_mapping"];
    assert!(mapping["price_amount"].as_str().is_some());
    assert!(mapping["price_currency"].as_str().is_some());
    assert!(mapping["seller_endpoint"].as_str().is_some());

    // Operations
    assert!(skill["provider"]["operations"]["search"]["path"]
        .as_str()
        .unwrap()
        .contains("/skill/search"));
    assert!(skill["provider"]["operations"]["lookup"]["path"]
        .as_str()
        .unwrap()
        .contains("/skill/item/"));
}
