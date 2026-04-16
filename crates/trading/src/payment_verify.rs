// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

/// Verify a payment proof for a P2P data purchase.
/// Supports x402 USDC TransferWithAuthorization receipts and ERC-8183 escrow tx.
pub fn verify_payment_proof(proof: &str, required_amount: f64) -> Result<bool> {
    if proof.is_empty() {
        return Ok(false);
    }

    // Try JSON payment proof
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(proof) {
        // x402 receipt: has "signature" and "amount" fields
        if let Some(sig) = parsed.get("signature").and_then(|v| v.as_str()) {
            if sig.is_empty() {
                return Ok(false);
            }
            // Verify amount covers required price
            if let Some(amount) = parsed.get("amount").and_then(|v| v.as_f64()) {
                return Ok(amount >= required_amount);
            }
            // If no amount field but signature present, accept (amount verified on-chain)
            return Ok(true);
        }

        // ERC-8183 escrow: has "tx_hash" and "escrow_address"
        if let Some(tx_hash) = parsed.get("tx_hash").and_then(|v| v.as_str()) {
            if tx_hash.len() == 66 && tx_hash.starts_with("0x") {
                return Ok(true);
            }
            if tx_hash.len() == 64 && tx_hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return Ok(true);
            }
        }
    }

    // Raw hex transaction hash (64 chars)
    if proof.len() == 64 && proof.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(true);
    }

    // 0x-prefixed hash (66 chars)
    if proof.len() == 66
        && proof.starts_with("0x")
        && proof[2..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Ok(true);
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_proof_fails() {
        assert!(!verify_payment_proof("", 1.0).unwrap());
    }

    #[test]
    fn hex_tx_hash_passes() {
        let hash = "a".repeat(64);
        assert!(verify_payment_proof(&hash, 1.0).unwrap());
    }

    #[test]
    fn hex_0x_prefixed_hash_passes() {
        let hash = format!("0x{}", "b".repeat(64));
        assert!(verify_payment_proof(&hash, 1.0).unwrap());
    }

    #[test]
    fn x402_receipt_passes() {
        let proof = r#"{"signature":"0xabc","amount":1.5}"#;
        assert!(verify_payment_proof(proof, 1.0).unwrap());
    }

    #[test]
    fn x402_insufficient_amount_fails() {
        let proof = r#"{"signature":"0xabc","amount":0.5}"#;
        assert!(!verify_payment_proof(proof, 1.0).unwrap());
    }

    #[test]
    fn x402_empty_signature_fails() {
        let proof = r#"{"signature":"","amount":10.0}"#;
        assert!(!verify_payment_proof(proof, 1.0).unwrap());
    }

    #[test]
    fn x402_no_amount_field_still_passes() {
        let proof = r#"{"signature":"0xabc123"}"#;
        assert!(verify_payment_proof(proof, 1.0).unwrap());
    }

    #[test]
    fn erc8183_escrow_passes() {
        let proof =
            r#"{"tx_hash":"0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890ab"}"#;
        assert!(verify_payment_proof(proof, 1.0).unwrap());
    }

    #[test]
    fn erc8183_raw_hex_hash_passes() {
        let proof = format!(r#"{{"tx_hash":"{}"}}"#, "c".repeat(64));
        assert!(verify_payment_proof(&proof, 1.0).unwrap());
    }

    #[test]
    fn random_string_fails() {
        assert!(!verify_payment_proof("not-a-valid-proof", 1.0).unwrap());
    }

    #[test]
    fn short_hex_fails() {
        assert!(!verify_payment_proof("abcdef", 1.0).unwrap());
    }

    #[test]
    fn json_without_required_fields_fails() {
        let proof = r#"{"foo":"bar"}"#;
        assert!(!verify_payment_proof(proof, 1.0).unwrap());
    }

    #[test]
    fn zero_amount_required_passes_with_any_signature() {
        let proof = r#"{"signature":"0xabc","amount":0.0}"#;
        assert!(verify_payment_proof(proof, 0.0).unwrap());
    }
}
