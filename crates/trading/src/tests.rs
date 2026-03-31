use crate::router::{PaymentRouter, TransactionContext};
use crate::wallet::AgentWallet;
use data_core::types::*;

fn test_wallet() -> AgentWallet {
    // Hardhat account #0; never use in production.
    AgentWallet::from_private_key(
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    )
    .unwrap()
}

fn make_ctx(
    amount: f64,
    single: bool,
    batch: bool,
    fiat: bool,
    verify: bool,
) -> TransactionContext {
    TransactionContext {
        buyer: Did("did:test:buyer".into()),
        seller: Did("did:test:seller".into()),
        dataset_cid: DatasetCid("cid-test".into()),
        amount,
        is_single_request: single,
        is_session_batch: batch,
        prefer_fiat: fiat,
        requires_verification: verify,
        seller_endpoint: None,
    }
}

// --- Wallet tests ---

#[test]
fn wallet_from_private_key_produces_deterministic_address() {
    let w1 = test_wallet();
    let w2 = test_wallet();
    assert_eq!(w1.address(), w2.address());
}

#[test]
fn wallet_from_invalid_key_fails() {
    assert!(AgentWallet::from_private_key("not-a-key").is_err());
}

#[tokio::test]
async fn wallet_sign_hash_returns_65_bytes() {
    let wallet = test_wallet();
    let hash = alloy_primitives::FixedBytes::ZERO;
    let sig = wallet.sign_hash(hash).await.unwrap();
    assert_eq!(sig.len(), 65); // r(32) + s(32) + v(1)
}

#[tokio::test]
async fn wallet_sign_message_returns_65_bytes() {
    let wallet = test_wallet();
    let sig = wallet.sign_message(b"hello").await.unwrap();
    assert_eq!(sig.len(), 65);
}

#[test]
fn eip712_hash_is_deterministic() {
    let wallet = test_wallet();
    let to: alloy_primitives::Address = "0x1234567890abcdef1234567890abcdef12345678"
        .parse()
        .unwrap();
    let token: alloy_primitives::Address = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"
        .parse()
        .unwrap();
    let amount = alloy_primitives::U256::from(10000u64);
    let nonce = alloy_primitives::U256::from(1u64);
    let deadline = alloy_primitives::U256::from(9999999u64);

    let h1 = wallet.eip712_transfer_hash(to, amount, nonce, deadline, token, 8453);
    let h2 = wallet.eip712_transfer_hash(to, amount, nonce, deadline, token, 8453);
    assert_eq!(h1, h2);

    // Different chain_id -> different hash
    let h3 = wallet.eip712_transfer_hash(to, amount, nonce, deadline, token, 84532);
    assert_ne!(h1, h3);
}

// --- Router protocol selection tests ---

#[test]
fn select_x402_for_micropayment() {
    let router = PaymentRouter::new(test_wallet(), true);
    let ctx = make_ctx(0.005, true, false, false, false);
    assert!(matches!(
        router.select_protocol(&ctx),
        PaymentProtocol::X402
    ));
}

#[test]
fn select_mpp_for_session_batch() {
    let router = PaymentRouter::new(test_wallet(), true);
    let ctx = make_ctx(0.50, false, true, false, false);
    assert!(matches!(
        router.select_protocol(&ctx),
        PaymentProtocol::StripeMpp
    ));
}

#[test]
fn select_mpp_for_fiat_preference() {
    let router = PaymentRouter::new(test_wallet(), true);
    let ctx = make_ctx(0.50, true, false, true, false);
    assert!(matches!(
        router.select_protocol(&ctx),
        PaymentProtocol::StripeMpp
    ));
}

#[test]
fn select_escrow_for_large_verified() {
    let router = PaymentRouter::new(test_wallet(), true);
    let ctx = make_ctx(5.0, false, false, false, true);
    assert!(matches!(
        router.select_protocol(&ctx),
        PaymentProtocol::Erc8183
    ));
}

#[test]
fn default_to_x402() {
    let router = PaymentRouter::new(test_wallet(), true);
    let ctx = make_ctx(0.50, false, false, false, false);
    assert!(matches!(
        router.select_protocol(&ctx),
        PaymentProtocol::X402
    ));
}

// --- MppSession tests ---

#[test]
fn mpp_session_tracks_budget() {
    let wallet = test_wallet();
    let client = crate::mpp::MppClient::new(wallet);
    let session = client.create_session(10.0);
    assert_eq!(session.budget_remaining, 10.0);
    assert!(!session.session_id.is_empty());
}
