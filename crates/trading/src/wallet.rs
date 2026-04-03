// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use alloy_primitives::{Address, FixedBytes, U256};
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Agent wallet for signing on-chain payments (USDC transfers).
#[derive(Clone)]
pub struct AgentWallet {
    signer: PrivateKeySigner,
}

impl AgentWallet {
    /// Load from a hex-encoded private key (without 0x prefix is fine).
    pub fn from_private_key(key: &str) -> Result<Self> {
        let signer: PrivateKeySigner = key.parse().context("invalid private key")?;
        Ok(Self { signer })
    }

    /// Load from a keyfile at the given path.
    pub fn from_keyfile(path: &Path) -> Result<Self> {
        let key = std::fs::read_to_string(path)
            .with_context(|| format!("read wallet key from {}", path.display()))?;
        Self::from_private_key(key.trim())
    }

    /// Load from the default keyfile at ~/.data-node/wallet.key.
    pub fn from_default_keyfile() -> Result<Self> {
        Self::from_keyfile(&Self::default_keyfile_path())
    }

    pub fn default_keyfile_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".data-node")
            .join("wallet.key")
    }

    pub fn address(&self) -> Address {
        self.signer.address()
    }

    /// Sign an EIP-712 typed-data hash (used by x402 exact scheme).
    pub async fn sign_hash(&self, hash: FixedBytes<32>) -> Result<Vec<u8>> {
        let sig = self.signer.sign_hash(&hash).await?;
        Ok(sig.as_bytes().to_vec())
    }

    /// Sign a raw message (used by MPP credential).
    pub async fn sign_message(&self, message: &[u8]) -> Result<Vec<u8>> {
        let sig = self.signer.sign_message(message).await?;
        Ok(sig.as_bytes().to_vec())
    }

    /// Compute EIP-712 hash for an ERC-20 transfer authorization.
    /// This is the "exact" scheme used by x402 on Base.
    pub fn eip712_transfer_hash(
        &self,
        to: Address,
        amount: U256,
        nonce: U256,
        deadline: U256,
        token: Address,
        chain_id: u64,
    ) -> FixedBytes<32> {
        use alloy_sol_types::sol;
        use alloy_sol_types::SolStruct;

        sol! {
            #[derive(Debug)]
            struct TransferWithAuthorization {
                address from;
                address to;
                uint256 value;
                uint256 validAfter;
                uint256 validBefore;
                bytes32 nonce;
            }
        }

        let msg = TransferWithAuthorization {
            from: self.address(),
            to,
            value: amount,
            validAfter: U256::ZERO,
            validBefore: deadline,
            nonce: FixedBytes::from(nonce.to_be_bytes::<32>()),
        };

        let domain = alloy_sol_types::eip712_domain! {
            name: "USD Coin",
            version: "2",
            chain_id: chain_id,
            verifying_contract: token,
        };

        msg.eip712_signing_hash(&domain)
    }
}
