#[cfg(test)]
mod tests {
    use crate::seller_reputation::{compute_tier, ReputationTier};
    use crate::buyer_review::{summarize_reviews, BuyerReview};
    use crate::client::BasescanTx;

    // --- Reputation tier tests ---

    #[test]
    fn tier_unknown_for_zero_sales() {
        assert_eq!(compute_tier(0, 0.0, 0), ReputationTier::Unknown);
    }

    #[test]
    fn tier_newcomer_for_few_sales() {
        assert_eq!(compute_tier(3, 0.05, 2), ReputationTier::Newcomer);
    }

    #[test]
    fn tier_established_for_moderate_activity() {
        assert_eq!(compute_tier(10, 0.5, 4), ReputationTier::Established);
    }

    #[test]
    fn tier_trusted_for_high_activity() {
        assert_eq!(compute_tier(25, 2.0, 8), ReputationTier::Trusted);
    }

    #[test]
    fn tier_not_trusted_without_enough_buyers() {
        // 25 sales, 2 ETH, but only 3 unique buyers → Established, not Trusted
        assert_eq!(compute_tier(25, 2.0, 3), ReputationTier::Established);
    }

    // --- Review summary tests ---

    fn make_review(rating: u8, comment: &str) -> BuyerReview {
        BuyerReview {
            tx_hash: "0xabc".into(),
            buyer: "0x123".into(),
            listing_id: "listing-1".into(),
            rating,
            comment: comment.into(),
            timestamp: 1700000000,
            value_eth: 0.01,
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
        let reviews = vec![make_review(5, "great"), make_review(3, "ok"), make_review(4, "good")];
        let summary = summarize_reviews("listing-1", &reviews);
        assert_eq!(summary.total_reviews, 3);
        assert!((summary.avg_rating - 4.0).abs() < 0.01);
    }

    // --- Review parsing from tx input ---

    #[test]
    fn parse_review_from_abi_with_trailing_bytes() {
        // Simulate ABI-encoded purchase("listing-1") + review payload
        // Selector (4) + offset (32) + length (32) + padded string (32) = 100 bytes
        let mut input = vec![0u8; 100];
        // selector
        input[0..4].copy_from_slice(&[0xab, 0xcd, 0xef, 0x01]);
        // offset = 32
        input[35] = 32;
        // string length = 9 ("listing-1")
        input[67] = 9;
        // string data "listing-1"
        input[68..77].copy_from_slice(b"listing-1");

        // Append review: rating=4, comment="nice data"
        input.push(4); // rating
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

        let review = super::super::buyer_review::parse_review_from_input(
            &tx.input, "listing-1", &tx,
        );
        assert!(review.is_some());
        let r = review.unwrap();
        assert_eq!(r.rating, 4);
        assert_eq!(r.comment, "nice data");
        assert_eq!(r.buyer, "0xbuyer"); // lowercased
        assert!((r.value_eth - 0.01).abs() < 0.0001);
    }

    #[test]
    fn parse_review_none_without_trailing_bytes() {
        // Exactly 100 bytes = standard ABI, no review
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

        let review = super::super::buyer_review::parse_review_from_input(
            &tx.input, "listing-1", &tx,
        );
        assert!(review.is_none());
    }

    // --- ChainConfig tests ---

    #[test]
    fn chain_config_base_mainnet() {
        let cfg = crate::ChainConfig::base_mainnet("0xContract");
        assert!(cfg.rpc_url.contains("mainnet.base.org"));
        assert!(cfg.explorer_api.contains("api.basescan.org"));
        assert_eq!(cfg.contract_address, "0xContract");
    }

    #[test]
    fn chain_config_base_sepolia() {
        let cfg = crate::ChainConfig::base_sepolia("0xTest");
        assert!(cfg.rpc_url.contains("sepolia.base.org"));
        assert!(cfg.explorer_api.contains("sepolia"));
    }
}
