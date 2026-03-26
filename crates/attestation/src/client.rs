use anyhow::Result;
use serde::Deserialize;

const BASE_MAINNET_RPC: &str = "https://mainnet.base.org";
const BASE_SEPOLIA_RPC: &str = "https://sepolia.base.org";
const BASESCAN_API: &str = "https://api.basescan.org/api";
const BASESCAN_SEPOLIA_API: &str = "https://api-sepolia.basescan.org/api";

#[derive(Debug, Clone)]
pub struct ChainConfig {
    pub rpc_url: String,
    pub explorer_api: String,
    pub explorer_api_key: Option<String>,
    pub contract_address: String,
}

impl ChainConfig {
    pub fn base_mainnet(contract: &str) -> Self {
        Self {
            rpc_url: BASE_MAINNET_RPC.into(),
            explorer_api: BASESCAN_API.into(),
            explorer_api_key: std::env::var("BASESCAN_API_KEY").ok(),
            contract_address: contract.into(),
        }
    }

    pub fn base_sepolia(contract: &str) -> Self {
        Self {
            rpc_url: BASE_SEPOLIA_RPC.into(),
            explorer_api: BASESCAN_SEPOLIA_API.into(),
            explorer_api_key: std::env::var("BASESCAN_API_KEY").ok(),
            contract_address: contract.into(),
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

impl BaseChainClient {
    pub fn new(config: ChainConfig) -> Self {
        Self { config, http: reqwest::Client::new() }
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
        let resp: BasescanResponse<Vec<BasescanTx>> = self.http
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        if resp.status != "1" {
            return Ok(vec![]);
        }
        Ok(resp.result)
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
