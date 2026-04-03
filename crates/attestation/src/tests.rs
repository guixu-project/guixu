// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use crate::buyer_review::{summarize_reviews, BuyerReview};
use crate::client::{BasescanTx, PaymentAmount, PaymentToken};
use crate::seller_reputation::{compute_tier, ReputationTier};

// --- Reputation tier tests (now USD-based thresholds) ---

#[test]
fn tier_unknown_for_zero_sales() {
    assert_eq!(compute_tier(0, 0.0, 0), ReputationTier::Unknown);
}

#[test]
fn tier_newcomer_for_few_sales() {
    assert_eq!(compute_tier(3, 5.0, 2), ReputationTier::Newcomer);
}

#[test]
fn tier_established_for_moderate_activity() {
    assert_eq!(compute_tier(10, 50.0, 4), ReputationTier::Established);
}

#[test]
fn tier_trusted_for_high_activity() {
    assert_eq!(compute_tier(25, 200.0, 8), ReputationTier::Trusted);
}

#[test]
fn tier_not_trusted_without_enough_buyers() {
    // 25 sales, $200, but only 3 unique buyers -> Established, not Trusted
    assert_eq!(compute_tier(25, 200.0, 3), ReputationTier::Established);
}

// --- PaymentToken tests ---

#[test]
fn eth_decimals_and_display() {
    assert_eq!(PaymentToken::ETH.decimals(), 18);
    let amt = PaymentToken::ETH.to_display_amount(10_000_000_000_000_000); // 0.01 ETH
    assert!((amt - 0.01).abs() < 0.0001);
}

#[test]
fn usdc_decimals_and_display() {
    assert_eq!(PaymentToken::USDC.decimals(), 6);
    let amt = PaymentToken::USDC.to_display_amount(1_500_000); // 1.5 USDC
    assert!((amt - 1.5).abs() < 0.0001);
}

#[test]
fn usdt_decimals_and_display() {
    assert_eq!(PaymentToken::USDT.decimals(), 6);
    let amt = PaymentToken::USDT.to_display_amount(10_000_000); // 10 USDT
    assert!((amt - 10.0).abs() < 0.0001);
}

// --- Review summary tests ---

fn make_review(rating: u8, comment: &str, token: PaymentToken, amount: f64) -> BuyerReview {
    BuyerReview {
        tx_hash: "0xabc".into(),
        buyer: "0x123".into(),
        listing_id: "listing-1".into(),
        rating,
        comment: comment.into(),
        timestamp: 1700000000,
        payment: PaymentAmount { token, amount },
    }
}

#[test]
fn summarize_empty_reviews() {
    let summary = summarize_reviews("listing-1", &[]);
    assert_eq!(summary.total_reviews, 0);
    assert_eq!(summary.avg_rating, 0.0);
}

#[test]
fn summarize_computes_average() {
    let reviews = vec![
        make_review(5, "great", PaymentToken::USDC, 10.0),
        make_review(3, "ok", PaymentToken::ETH, 0.01),
        make_review(4, "good", PaymentToken::USDT, 5.0),
    ];
    let summary = summarize_reviews("listing-1", &reviews);
    assert_eq!(summary.total_reviews, 3);
    assert!((summary.avg_rating - 4.0).abs() < 0.01);
}

// --- Review parsing from tx input ---

#[test]
fn parse_review_from_abi_with_trailing_bytes() {
    let mut input = vec![0u8; 100];
    input[0..4].copy_from_slice(&[0xab, 0xcd, 0xef, 0x01]);
    input[35] = 32;
    input[67] = 9;
    input[68..77].copy_from_slice(b"listing-1");

    // Append review: rating=4, comment="nice data"
    input.push(4);
    input.extend_from_slice(b"nice data");

    let input_hex = format!("0x{}", hex::encode(&input));

    let tx = BasescanTx {
        hash: "0xtx1".into(),
        from: "0xBuyer".into(),
        to: "0xContract".into(),
        value_wei: "10000000000000000".into(), // 0.01 ETH
        timestamp: "1700000000".into(),
        input: input_hex,
        is_error: "0".into(),
        function_name: Some("purchase(string)".into()),
    };

    let token_payments = std::collections::HashMap::new();
    let review =
        super::buyer_review::parse_review_from_input(&tx.input, "listing-1", &tx, &token_payments);
    assert!(review.is_some());
    let r = review.unwrap();
    assert_eq!(r.rating, 4);
    assert_eq!(r.comment, "nice data");
    assert_eq!(r.buyer, "0xbuyer");
    assert_eq!(r.payment.token, PaymentToken::ETH);
    assert!((r.payment.amount - 0.01).abs() < 0.0001);
}

#[test]
fn parse_review_with_usdc_payment() {
    let mut input = vec![0u8; 100];
    input[0..4].copy_from_slice(&[0xab, 0xcd, 0xef, 0x01]);
    input[35] = 32;
    input[67] = 9;
    input[68..77].copy_from_slice(b"listing-1");
    input.push(5);
    input.extend_from_slice(b"excellent");

    let input_hex = format!("0x{}", hex::encode(&input));

    let tx = BasescanTx {
        hash: "0xtx_usdc".into(),
        from: "0xBuyer".into(),
        to: "0xContract".into(),
        value_wei: "0".into(), // no ETH - paid with USDC
        timestamp: "1700000000".into(),
        input: input_hex,
        is_error: "0".into(),
        function_name: Some("purchase(string)".into()),
    };

    let mut token_payments = std::collections::HashMap::new();
    token_payments.insert(
        "0xtx_usdc".to_string(),
        PaymentAmount {
            token: PaymentToken::USDC,
            amount: 25.0,
        },
    );

    let review =
        super::buyer_review::parse_review_from_input(&tx.input, "listing-1", &tx, &token_payments);
    assert!(review.is_some());
    let r = review.unwrap();
    assert_eq!(r.payment.token, PaymentToken::USDC);
    assert!((r.payment.amount - 25.0).abs() < 0.01);
}

#[test]
fn parse_review_none_without_trailing_bytes() {
    let mut input = vec![0u8; 100];
    input[35] = 32;
    input[67] = 9;
    input[68..77].copy_from_slice(b"listing-1");

    let input_hex = format!("0x{}", hex::encode(&input));
    let tx = BasescanTx {
        hash: "0xtx".into(),
        from: "0xB".into(),
        to: "0xC".into(),
        value_wei: "0".into(),
        timestamp: "0".into(),
        input: input_hex,
        is_error: "0".into(),
        function_name: Some("purchase(string)".into()),
    };

    let token_payments = std::collections::HashMap::new();
    let review =
        super::buyer_review::parse_review_from_input(&tx.input, "listing-1", &tx, &token_payments);
    assert!(review.is_none());
}

// --- ChainConfig tests ---

#[test]
fn chain_config_base_mainnet() {
    let cfg = crate::ChainConfig::base_mainnet("0xContract");
    assert!(cfg.rpc_url.contains("mainnet.base.org"));
    assert!(cfg.explorer_api.contains("api.basescan.org"));
    assert_eq!(cfg.contract_address, "0xContract");
    assert!(!cfg.tokens.usdc.is_empty());
    assert!(!cfg.tokens.usdt.is_empty());
}

#[test]
fn chain_config_base_sepolia() {
    let cfg = crate::ChainConfig::base_sepolia("0xTest");
    assert!(cfg.rpc_url.contains("sepolia.base.org"));
    assert!(cfg.explorer_api.contains("sepolia"));
    assert!(!cfg.tokens.usdc.is_empty());
    assert!(!cfg.tokens.usdt.is_empty());
}

#[test]
fn identify_token_usdc_mainnet() {
    let cfg = crate::ChainConfig::base_mainnet("0xContract");
    assert_eq!(
        cfg.identify_token("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"),
        Some(PaymentToken::USDC),
    );
}

#[test]
fn identify_token_usdt_mainnet() {
    let cfg = crate::ChainConfig::base_mainnet("0xContract");
    assert_eq!(
        cfg.identify_token("0xfde4C96c8593536E31F229EA8f37b2ADa2699bb2"),
        Some(PaymentToken::USDT),
    );
}

#[test]
fn identify_token_unknown() {
    let cfg = crate::ChainConfig::base_mainnet("0xContract");
    assert_eq!(cfg.identify_token("0xUnknownToken"), None);
}
