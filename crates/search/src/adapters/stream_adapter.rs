// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::pin::Pin;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use data_core::types::{SkillCapability, SourceFamily};
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

pub type BoxStream = Pin<Box<dyn Stream<Item = SignalEvent> + Send + Sync>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEvent {
    pub signal_id: SignalId,
    pub signal_family: SignalFamily,
    pub chain_id: u64,
    pub block_number: u64,
    pub tx_hash: TxHash,
    pub observed_at: DateTime<Utc>,
    pub source: SignalSource,
    pub entity_refs: EntityRefs,
    pub features: HashMap<String, f64>,
    pub evidence: Vec<Evidence>,
    pub confidence: f64,
    pub freshness_ms: u64,
    pub reorg_safe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalId(pub String);

impl SignalId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalFamily {
    Mempool,
    Swap,
    Bridge,
    Mint,
    Governance,
    ContractVerify,
    WhaleFlow,
    NewPair,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxHash(pub String);

impl TxHash {
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSource {
    pub adapter_name: String,
    pub endpoint: String,
    pub subscription_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityRefs {
    pub tokens: Vec<EntityRef>,
    pub pools: Vec<EntityRef>,
    pub wallets: Vec<EntityRef>,
    pub contracts: Vec<EntityRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRef {
    pub entity_type: EntityType,
    pub chain_id: u64,
    pub address: String,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Pool,
    Vault,
    Bridge,
    Minter,
    Deployer,
    Lp,
    Token,
    Wallet,
    Contract,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub evidence_type: EvidenceType,
    pub description: String,
    pub raw_data: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    SwapTrace,
    TransferTrace,
    ApprovalTrace,
    ContractCreation,
    GovernanceVote,
    PriceImpact,
    VolumeSpike,
    WalletCluster,
}

#[async_trait::async_trait]
pub trait StreamAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn skill_id(&self) -> &str;
    fn source_family(&self) -> SourceFamily;
    fn capabilities(&self) -> Vec<SkillCapability>;

    async fn subscribe(&self, query: &str) -> Result<BoxStream>;
    async fn unsubscribe(&self, subscription_id: &str) -> Result<()>;
    async fn backfill(
        &self,
        query: &str,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<SignalEvent>>;
    async fn decode(&self, raw_data: &[u8]) -> Result<SignalEvent>;
}

#[derive(Debug, Clone)]
pub struct WebSocketStreamAdapterConfig {
    pub name: String,
    pub skill_id: String,
    pub endpoint: Url,
    pub subscribe_path: String,
    pub unsubscribe_path: String,
    pub auth: Option<WsAuth>,
}

#[derive(Debug, Clone)]
pub enum WsAuth {
    Bearer(String),
    Basic { username: String, password: String },
}

pub struct WebSocketStreamAdapter {
    config: WebSocketStreamAdapterConfig,
}

impl WebSocketStreamAdapter {
    pub fn new(config: WebSocketStreamAdapterConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl StreamAdapter for WebSocketStreamAdapter {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn skill_id(&self) -> &str {
        &self.config.skill_id
    }

    fn source_family(&self) -> SourceFamily {
        SourceFamily::Custom
    }

    fn capabilities(&self) -> Vec<SkillCapability> {
        vec![
            SkillCapability::Subscribe,
            SkillCapability::Backfill,
            SkillCapability::Decode,
        ]
    }

    async fn subscribe(&self, _query: &str) -> Result<BoxStream> {
        let (ws_stream, _) = connect_async(self.config.endpoint.as_str()).await?;
        let read = ws_stream.filter_map(
            |msg: Result<Message, tokio_tungstenite::tungstenite::Error>| async move {
                match msg {
                    Ok(Message::Text(text)) => Some(text.to_string()),
                    Ok(Message::Binary(data)) => String::from_utf8(data.to_vec()).ok(),
                    _ => None,
                }
            },
        );

        let stream = read
            .map(|data: String| -> Result<SignalEvent> {
                let value: serde_json::Value = serde_json::from_str(&data)?;
                parse_jsonrpc_value(&value)
            })
            .filter_map(|r: Result<SignalEvent>| async move { r.ok() });

        Ok(Box::pin(stream))
    }

    async fn unsubscribe(&self, _subscription_id: &str) -> Result<()> {
        Ok(())
    }

    async fn backfill(
        &self,
        _query: &str,
        _from_block: u64,
        _to_block: u64,
    ) -> Result<Vec<SignalEvent>> {
        Err(anyhow!("backfill not implemented for WebSocket adapter"))
    }

    async fn decode(&self, raw_data: &[u8]) -> Result<SignalEvent> {
        let text = String::from_utf8(raw_data.to_vec())?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        parse_jsonrpc_value(&value)
    }
}

fn parse_jsonrpc_value(value: &serde_json::Value) -> Result<SignalEvent> {
    if let (Some(params), Some(_method)) = (value.get("params"), value.get("method")) {
        let result = params.get("result").or(Some(params)).unwrap_or(value);
        return json_value_to_signal_event(result);
    }
    if value.get("result").is_some() {
        let result = value.get("result").unwrap();
        return json_value_to_signal_event(result);
    }
    json_value_to_signal_event(value)
}

fn json_value_to_signal_event(value: &serde_json::Value) -> Result<SignalEvent> {
    let signal_id = value
        .get("signal_id")
        .and_then(|v| v.as_str())
        .map(SignalId::new)
        .unwrap_or_else(|| SignalId::new(uuid::Uuid::new_v4().to_string()));

    let signal_family = value
        .get("signal_family")
        .and_then(|v| v.as_str())
        .and_then(|s| match s {
            "mempool" => Some(SignalFamily::Mempool),
            "swap" => Some(SignalFamily::Swap),
            "bridge" => Some(SignalFamily::Bridge),
            "mint" => Some(SignalFamily::Mint),
            "governance" => Some(SignalFamily::Governance),
            "contract_verify" => Some(SignalFamily::ContractVerify),
            "whale_flow" => Some(SignalFamily::WhaleFlow),
            "new_pair" => Some(SignalFamily::NewPair),
            _ => None,
        })
        .unwrap_or(SignalFamily::Mempool);

    let chain_id = value.get("chain_id").and_then(|v| v.as_u64()).unwrap_or(1);

    let block_number = value
        .get("block_number")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let tx_hash = value
        .get("tx_hash")
        .and_then(|v| v.as_str())
        .map(TxHash::new)
        .unwrap_or_else(|| TxHash::new("0x0"));

    let observed_at = value
        .get("observed_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let source = SignalSource {
        adapter_name: "websocket".to_string(),
        endpoint: String::new(),
        subscription_id: None,
    };

    let features: HashMap<String, f64> = value
        .get("features")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
                .collect()
        })
        .unwrap_or_default();

    let evidence: Vec<Evidence> = value
        .get("evidence")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    Some(Evidence {
                        evidence_type: EvidenceType::SwapTrace,
                        description: v.get("description")?.as_str()?.to_string(),
                        raw_data: v.get("raw_data").and_then(|d| d.as_str()).map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let confidence = value
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let freshness_ms = value
        .get("freshness_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let reorg_safe = value
        .get("reorg_safe")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(SignalEvent {
        signal_id,
        signal_family,
        chain_id,
        block_number,
        tx_hash,
        observed_at,
        source,
        entity_refs: EntityRefs::default(),
        features,
        evidence,
        confidence,
        freshness_ms,
        reorg_safe,
    })
}

#[derive(Debug, Clone)]
pub struct GrpcStreamAdapterConfig {
    pub name: String,
    pub skill_id: String,
    pub endpoint: String,
    pub service_name: String,
    pub rpc_name: String,
}

pub struct GrpcStreamAdapter {
    config: GrpcStreamAdapterConfig,
}

impl GrpcStreamAdapter {
    pub fn new(config: GrpcStreamAdapterConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl StreamAdapter for GrpcStreamAdapter {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn skill_id(&self) -> &str {
        &self.config.skill_id
    }

    fn source_family(&self) -> SourceFamily {
        SourceFamily::Custom
    }

    fn capabilities(&self) -> Vec<SkillCapability> {
        vec![
            SkillCapability::Subscribe,
            SkillCapability::Backfill,
            SkillCapability::Decode,
        ]
    }

    async fn subscribe(&self, _query: &str) -> Result<BoxStream> {
        Err(anyhow!("gRPC streaming not yet implemented"))
    }

    async fn unsubscribe(&self, _subscription_id: &str) -> Result<()> {
        Ok(())
    }

    async fn backfill(
        &self,
        _query: &str,
        _from_block: u64,
        _to_block: u64,
    ) -> Result<Vec<SignalEvent>> {
        Err(anyhow!("backfill not implemented for gRPC adapter"))
    }

    async fn decode(&self, _raw_data: &[u8]) -> Result<SignalEvent> {
        Err(anyhow!("gRPC decoding not yet implemented"))
    }
}

#[derive(Debug, Clone)]
pub struct SubgraphStreamAdapterConfig {
    pub name: String,
    pub skill_id: String,
    pub subgraph_url: String,
    pub poll_interval_ms: u64,
    pub query_template: String,
}

pub struct SubgraphStreamAdapter {
    config: SubgraphStreamAdapterConfig,
    client: reqwest::Client,
}

impl SubgraphStreamAdapter {
    pub fn new(config: SubgraphStreamAdapterConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl StreamAdapter for SubgraphStreamAdapter {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn skill_id(&self) -> &str {
        &self.config.skill_id
    }

    fn source_family(&self) -> SourceFamily {
        SourceFamily::WebRegistry
    }

    fn capabilities(&self) -> Vec<SkillCapability> {
        vec![
            SkillCapability::Subscribe,
            SkillCapability::Backfill,
            SkillCapability::Decode,
        ]
    }

    async fn subscribe(&self, query: &str) -> Result<BoxStream> {
        let this = self.clone();
        let query = query.to_string();
        let poll_interval = self.config.poll_interval_ms;

        let stream = futures::stream::unfold((), move |_| {
            let this = this.clone();
            let query = query.clone();
            async move {
                let result = this.poll_once(&query).await.ok();
                tokio::time::sleep(std::time::Duration::from_millis(poll_interval)).await;
                result.map(|event| (event, ()))
            }
        });

        Ok(Box::pin(stream))
    }

    async fn unsubscribe(&self, _subscription_id: &str) -> Result<()> {
        Ok(())
    }

    async fn backfill(
        &self,
        query: &str,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<SignalEvent>> {
        let mut all_events = Vec::new();
        let mut current_block = from_block;
        let block_step = 1000u64;

        while current_block < to_block {
            let end_block = (current_block + block_step).min(to_block);
            let result = self.fetch_events(query, current_block, end_block).await?;
            all_events.extend(result);
            current_block = end_block;
        }

        Ok(all_events)
    }

    async fn decode(&self, raw_data: &[u8]) -> Result<SignalEvent> {
        let text = String::from_utf8(raw_data.to_vec())?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        self.parse_subgraph_event(&value)
    }
}

impl SubgraphStreamAdapter {
    async fn poll_once(&self, query: &str) -> Result<SignalEvent> {
        let response = self
            .client
            .post(&self.config.subgraph_url)
            .json(&serde_json::json!({
                "query": query
            }))
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;
        self.parse_subgraph_event(&data)
    }

    async fn fetch_events(
        &self,
        query: &str,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<SignalEvent>> {
        let query_with_blocks = format!(
            "{} {{ block: {{ number_gte: {}, number_lte: {} }} }}",
            query.trim_end_matches('}'),
            from_block,
            to_block
        );

        let response = self
            .client
            .post(&self.config.subgraph_url)
            .json(&serde_json::json!({
                "query": query_with_blocks
            }))
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;
        self.parse_subgraph_array(&data)
    }

    fn parse_subgraph_event(&self, value: &serde_json::Value) -> Result<SignalEvent> {
        let events = self.parse_subgraph_array(value)?;
        events
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no events in response"))
    }

    fn parse_subgraph_array(&self, value: &serde_json::Value) -> Result<Vec<SignalEvent>> {
        let data = value
            .get("data")
            .ok_or_else(|| anyhow!("missing data field"))?;
        let first_key = data
            .as_object()
            .and_then(|obj| obj.keys().next())
            .ok_or_else(|| anyhow!("no keys in data"))?;

        let items = data
            .get(first_key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let events = items
            .into_iter()
            .map(|item| SignalEvent {
                signal_id: SignalId::new(
                    item.get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&uuid::Uuid::new_v4().to_string())
                        .to_string(),
                ),
                signal_family: SignalFamily::Swap,
                chain_id: 1,
                block_number: item
                    .get("blockNumber")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                tx_hash: item
                    .get("transaction")
                    .and_then(|t| t.get("id"))
                    .and_then(|id| id.as_str())
                    .map(TxHash::new)
                    .unwrap_or_else(|| TxHash::new("0x0")),
                observed_at: item
                    .get("timestamp")
                    .and_then(|v| v.as_i64())
                    .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                    .unwrap_or_else(Utc::now),
                source: SignalSource {
                    adapter_name: self.config.name.clone(),
                    endpoint: self.config.subgraph_url.clone(),
                    subscription_id: None,
                },
                entity_refs: EntityRefs::default(),
                features: HashMap::new(),
                evidence: vec![],
                confidence: 1.0,
                freshness_ms: 0,
                reorg_safe: false,
            })
            .collect();

        Ok(events)
    }
}

impl Clone for SubgraphStreamAdapter {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            client: self.client.clone(),
        }
    }
}
