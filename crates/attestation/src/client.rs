// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde::{Deserialize, Serialize};

use data_core::config::BlockchainConfig;

/// Supported payment tokens on Base chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaymentToken {
    ETH,
    USDC,
    USDT,
}

impl PaymentToken {
    pub fn decimals(&self) -> u8 {
        match self {
            Self::ETH => 18,
            Self::USDC | Self::USDT => 6,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Self::ETH => "ETH",
            Self::USDC => "USDC",
            Self::USDT => "USDT",
        }
    }

    /// Convert raw token amount (smallest unit) to human-readable value.
    pub fn to_display_amount(&self, raw: u128) -> f64 {
        raw as f64 / 10f64.powi(self.decimals() as i32)
    }
}

/// A payment amount with its token type.
#[derive(Debug, Clone, Serialize)]
pub struct PaymentAmount {
    pub token: PaymentToken,
    pub amount: f64,
}

#[derive(Debug, Clone)]
pub struct TokenAddresses {
    pub usdc: String,
    pub usdt: String,
}

#[derive(Debug, Clone)]
pub struct ChainConfig {
    pub rpc_url: String,
    pub explorer_api: String,
    pub explorer_api_key: Option<String>,
    pub contract_address: String,
    pub tokens: TokenAddresses,
}

impl ChainConfig {
    /// Construct ChainConfig from BlockchainConfig.
    pub fn from_blockchain_config(config: &BlockchainConfig, contract_address: &str) -> Self {
        Self {
            rpc_url: config.rpc_url().to_string(),
            explorer_api: config.explorer_api_url().to_string(),
            explorer_api_key: config.basescan_api_key.clone(),
            contract_address: contract_address.to_string(),
            tokens: TokenAddresses {
                usdc: config.usdc_address.clone(),
                usdt: config.usdt_address.clone(),
            },
        }
    }

    /// Identify token type from a contract address.
    pub fn identify_token(&self, contract_address: &str) -> Option<PaymentToken> {
        let addr = contract_address.to_lowercase();
        if addr == self.tokens.usdc.to_lowercase() {
            Some(PaymentToken::USDC)
        } else if addr == self.tokens.usdt.to_lowercase() {
            Some(PaymentToken::USDT)
        } else {
            None
        }
    }
}

pub struct BaseChainClient {
    pub config: ChainConfig,
    http: reqwest::Client,
}

/// Basescan API response wrapper.
#[derive(Debug, Deserialize)]
struct BasescanResponse<T> {
    status: String,
    result: T,
}

/// A single transaction from Basescan.
#[derive(Debug, Clone, Deserialize)]
pub struct BasescanTx {
    pub hash: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "value")]
    pub value_wei: String,
    #[serde(rename = "timeStamp")]
    pub timestamp: String,
    pub input: String,
    #[serde(rename = "isError")]
    pub is_error: String,
    #[serde(rename = "functionName")]
    pub function_name: Option<String>,
}

/// An ERC-20 token transfer from Basescan tokentx API.
#[derive(Debug, Clone, Deserialize)]
pub struct BasescanTokenTx {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: String,
    #[serde(rename = "timeStamp")]
    pub timestamp: String,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "tokenSymbol")]
    pub token_symbol: String,
    #[serde(rename = "tokenDecimal")]
    pub token_decimal: String,
}

impl BaseChainClient {
    pub fn new(config: ChainConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    /// Create a BaseChainClient from BlockchainConfig.
    pub fn from_blockchain_config(
        blockchain_config: &BlockchainConfig,
        contract_address: &str,
    ) -> Self {
        Self::new(ChainConfig::from_blockchain_config(
            blockchain_config,
            contract_address,
        ))
    }

    /// Fetch normal transactions for an address from Basescan.
    pub async fn get_transactions(
        &self,
        address: &str,
        start_block: u64,
        end_block: u64,
    ) -> Result<Vec<BasescanTx>> {
        let api_key = self.config.explorer_api_key.as_deref().unwrap_or("");
        let url = format!(
            "{}?module=account&action=txlist&address={}&startblock={}&endblock={}&sort=desc&apikey={}",
            self.config.explorer_api, address, start_block, end_block, api_key,
        );
        let resp: BasescanResponse<Vec<BasescanTx>> =
            self.http.get(&url).send().await?.json().await?;

        if resp.status != "1" {
            return Ok(vec![]);
        }
        Ok(resp.result)
    }

    /// Fetch ERC-20 token transfers for an address from Basescan.
    pub async fn get_token_transfers(
        &self,
        address: &str,
        contract_address: Option<&str>,
        start_block: u64,
        end_block: u64,
    ) -> Result<Vec<BasescanTokenTx>> {
        let api_key = self.config.explorer_api_key.as_deref().unwrap_or("");
        let mut url = format!(
            "{}?module=account&action=tokentx&address={}&startblock={}&endblock={}&sort=desc&apikey={}",
            self.config.explorer_api, address, start_block, end_block, api_key,
        );
        if let Some(ca) = contract_address {
            url.push_str(&format!("&contractaddress={}", ca));
        }
        let resp: BasescanResponse<Vec<BasescanTokenTx>> =
            self.http.get(&url).send().await?.json().await?;

        if resp.status != "1" {
            return Ok(vec![]);
        }
        Ok(resp.result)
    }

    /// Fetch USDC and USDT transfers related to the DataTrade contract.
    pub async fn get_contract_token_transfers(
        &self,
        address: &str,
    ) -> Result<Vec<BasescanTokenTx>> {
        let all = self.get_token_transfers(address, None, 0, 99999999).await?;
        let contract_lower = self.config.contract_address.to_lowercase();
        let usdc_lower = self.config.tokens.usdc.to_lowercase();
        let usdt_lower = self.config.tokens.usdt.to_lowercase();
        Ok(all
            .into_iter()
            .filter(|tx| {
                let ca = tx.contract_address.to_lowercase();
                (ca == usdc_lower || ca == usdt_lower)
                    && (tx.from.to_lowercase() == contract_lower
                        || tx.to.to_lowercase() == contract_lower)
            })
            .collect())
    }

    /// Fetch transactions specifically to the DataTrade contract.
    pub async fn get_contract_transactions(&self, address: &str) -> Result<Vec<BasescanTx>> {
        let all = self.get_transactions(address, 0, 99999999).await?;
        let contract_lower = self.config.contract_address.to_lowercase();
        Ok(all
            .into_iter()
            .filter(|tx| tx.to.to_lowercase() == contract_lower)
            .collect())
    }
}
