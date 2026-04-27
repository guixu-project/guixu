// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::types::{
    AlphaScore, ChainId, EntityRefs, ExecutionAction, ExecutionIntent, Opportunity, SignalEvent,
    SignalFamily, SignalId, SignalSource, TxHash,
};
use serde_json::{json, Value};
use uuid::Uuid;

use super::trace_hooks::with_trace;
use crate::server::AppState;

#[derive(Debug, Clone, Default)]
pub struct RankFilters {
    pub signal_families: Vec<SignalFamily>,
    pub chain_ids: Vec<ChainId>,
    pub min_alpha_score: Option<f64>,
    pub action_mode: Option<ExecutionAction>,
}

fn parse_signal_families(args: &Value) -> Vec<SignalFamily> {
    args.get("signal_families")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_str().and_then(|s| match s {
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
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_chain_ids(args: &Value) -> Vec<ChainId> {
    args.get("chain_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| ChainId(s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_min_alpha_score(args: &Value) -> Option<f64> {
    args.get("min_alpha_score")
        .and_then(|v| v.as_f64())
        .filter(|&score| (0.0..=100.0).contains(&score))
}

fn parse_action_mode(args: &Value) -> Option<ExecutionAction> {
    args.get("action_mode").and_then(|v| {
        v.as_str().and_then(|s| match s {
            "alert" => Some(ExecutionAction::Alert),
            "simulate" => Some(ExecutionAction::Simulate),
            "semi_auto" => Some(ExecutionAction::SemiAuto),
            "auto_execute" => Some(ExecutionAction::AutoExecute),
            _ => None,
        })
    })
}

fn parse_limit(args: &Value) -> usize {
    args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize
}

fn get_sample_opportunities() -> Vec<Opportunity> {
    let now = chrono::Utc::now();
    vec![
        Opportunity {
            opportunity_id: Uuid::new_v4(),
            signal_events: vec![SignalEvent {
                signal_id: SignalId("sig_001".to_string()),
                signal_family: SignalFamily::Swap,
                chain_id: ChainId("ethereum".to_string()),
                block_number: 19500000,
                tx_hash: TxHash("0xabc123".to_string()),
                observed_at: now,
                source: SignalSource {
                    skill_id: "uniswap_v3".to_string(),
                    adapter_kind: "subgraph".to_string(),
                    endpoint: "https://api.thegraph.com".to_string(),
                },
                entity_refs: EntityRefs {
                    tokens: vec!["USDC".to_string(), "WETH".to_string()],
                    pools: vec!["0x1234...".to_string()],
                    wallets: vec![],
                    contracts: vec![],
                },
                features: [("amount_usd", 125000.0), ("price_impact_bps", 50.0)]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                evidence: vec![],
                confidence: 0.92,
                freshness_ms: 500,
                reorg_safe: true,
            }],
            alpha_score: AlphaScore {
                freshness_score: 95.0,
                novelty_score: 88.0,
                entity_importance_score: 78.0,
                flow_score: 92.0,
                execution_score: 85.0,
                risk_score: 15.0,
                evidence_score: 90.0,
                decay_score: 80.0,
                total: 82.5,
            },
            execution_plan: Some(ExecutionIntent {
                action: ExecutionAction::Simulate,
                route: vec!["uniswap_v3".to_string()],
                gas_budget: Some(300000),
                slippage_bps: Some(50),
            }),
            created_at: now,
            expires_at: Some(now + chrono::Duration::minutes(30)),
        },
        Opportunity {
            opportunity_id: Uuid::new_v4(),
            signal_events: vec![SignalEvent {
                signal_id: SignalId("sig_002".to_string()),
                signal_family: SignalFamily::Mempool,
                chain_id: ChainId("ethereum".to_string()),
                block_number: 19500001,
                tx_hash: TxHash("0xdef456".to_string()),
                observed_at: now,
                source: SignalSource {
                    skill_id: "mempool_feed".to_string(),
                    adapter_kind: "ws_jsonrpc".to_string(),
                    endpoint: "wss://mainnet.infura.io".to_string(),
                },
                entity_refs: EntityRefs {
                    tokens: vec!["USDC".to_string()],
                    pools: vec![],
                    wallets: vec!["0x whale...".to_string()],
                    contracts: vec![],
                },
                features: [("gas_price_gwei", 45.0), ("value_usd", 500000.0)]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                evidence: vec![],
                confidence: 0.88,
                freshness_ms: 200,
                reorg_safe: false,
            }],
            alpha_score: AlphaScore {
                freshness_score: 98.0,
                novelty_score: 75.0,
                entity_importance_score: 95.0,
                flow_score: 88.0,
                execution_score: 70.0,
                risk_score: 35.0,
                evidence_score: 82.0,
                decay_score: 90.0,
                total: 78.4,
            },
            execution_plan: Some(ExecutionIntent {
                action: ExecutionAction::Alert,
                route: vec![],
                gas_budget: None,
                slippage_bps: None,
            }),
            created_at: now,
            expires_at: Some(now + chrono::Duration::minutes(5)),
        },
        Opportunity {
            opportunity_id: Uuid::new_v4(),
            signal_events: vec![SignalEvent {
                signal_id: SignalId("sig_003".to_string()),
                signal_family: SignalFamily::NewPair,
                chain_id: ChainId("polygon".to_string()),
                block_number: 55000000,
                tx_hash: TxHash("0xghi789".to_string()),
                observed_at: now,
                source: SignalSource {
                    skill_id: "dex_screener".to_string(),
                    adapter_kind: "websocket".to_string(),
                    endpoint: "wss://api.dexscreener.com".to_string(),
                },
                entity_refs: EntityRefs {
                    tokens: vec!["NEWTOKEN".to_string(), "USDC".to_string()],
                    pools: vec!["0xnewpool...".to_string()],
                    wallets: vec![],
                    contracts: vec![],
                },
                features: [("initial_liquidity_usd", 50000.0), ("honeypot_risk", 0.1)]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                evidence: vec![],
                confidence: 0.72,
                freshness_ms: 1000,
                reorg_safe: true,
            }],
            alpha_score: AlphaScore {
                freshness_score: 99.0,
                novelty_score: 98.0,
                entity_importance_score: 45.0,
                flow_score: 65.0,
                execution_score: 55.0,
                risk_score: 60.0,
                evidence_score: 70.0,
                decay_score: 95.0,
                total: 68.5,
            },
            execution_plan: Some(ExecutionIntent {
                action: ExecutionAction::SemiAuto,
                route: vec!["quick_swap".to_string()],
                gas_budget: Some(200000),
                slippage_bps: Some(100),
            }),
            created_at: now,
            expires_at: Some(now + chrono::Duration::hours(1)),
        },
    ]
}

fn filter_opportunities(opps: Vec<Opportunity>, filters: &RankFilters) -> Vec<Opportunity> {
    opps.into_iter()
        .filter(|opp| {
            if !filters.signal_families.is_empty() {
                let has_matching_family = opp
                    .signal_events
                    .iter()
                    .any(|se| filters.signal_families.contains(&se.signal_family));
                if !has_matching_family {
                    return false;
                }
            }
            if !filters.chain_ids.is_empty() {
                let has_matching_chain = opp
                    .signal_events
                    .iter()
                    .any(|se| filters.chain_ids.contains(&se.chain_id));
                if !has_matching_chain {
                    return false;
                }
            }
            if let Some(min_score) = filters.min_alpha_score {
                if opp.alpha_score.total < min_score {
                    return false;
                }
            }
            if let Some(action_mode) = &filters.action_mode {
                if let Some(ref plan) = opp.execution_plan {
                    if &plan.action != action_mode {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        })
        .collect()
}

fn format_opportunities(opps: Vec<Opportunity>) -> Vec<Value> {
    opps.into_iter()
        .map(|opp| {
            let primary_signal = opp.signal_events.first();
            let signal_family = primary_signal
                .map(|s| format!("{:?}", s.signal_family).to_lowercase())
                .unwrap_or_else(|| "unknown".to_string());
            let chain_id = primary_signal
                .map(|s| s.chain_id.0.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let block_number = primary_signal.map(|s| s.block_number).unwrap_or(0);
            let tx_hash = primary_signal
                .map(|s| s.tx_hash.0.clone())
                .unwrap_or_else(|| "unknown".to_string());

            let evidence_summary: Vec<Value> = opp
                .signal_events
                .iter()
                .flat_map(|se| se.evidence.iter())
                .map(|e| json!({ "type": e.evidence_type, "description": e.description }))
                .collect();

            let action_str = opp
                .execution_plan
                .as_ref()
                .map(|p| format!("{:?}", p.action).to_lowercase())
                .unwrap_or_else(|| "none".to_string());

            json!({
                "rank": 0,
                "opportunity_id": opp.opportunity_id.to_string(),
                "signal_family": signal_family,
                "chain_id": chain_id,
                "alpha_score": {
                    "total": format!("{:.2}", opp.alpha_score.total),
                    "freshness": format!("{:.1}", opp.alpha_score.freshness_score),
                    "novelty": format!("{:.1}", opp.alpha_score.novelty_score),
                    "entity_importance": format!("{:.1}", opp.alpha_score.entity_importance_score),
                    "flow": format!("{:.1}", opp.alpha_score.flow_score),
                    "execution": format!("{:.1}", opp.alpha_score.execution_score),
                    "risk": format!("{:.1}", opp.alpha_score.risk_score),
                    "evidence": format!("{:.1}", opp.alpha_score.evidence_score),
                    "decay": format!("{:.1}", opp.alpha_score.decay_score),
                },
                "action_mode": action_str,
                "block_number": block_number,
                "tx_hash": tx_hash,
                "confidence": primary_signal.map(|s| format!("{:.0}%", s.confidence * 100.0)).unwrap_or_else(|| "N/A".to_string()),
                "freshness_ms": primary_signal.map(|s| s.freshness_ms).unwrap_or(0),
                "reorg_safe": primary_signal.map(|s| s.reorg_safe).unwrap_or(false),
                "evidence_count": evidence_summary.len(),
                "evidence": evidence_summary,
                "features": primary_signal.map(|s| s.features.clone()).unwrap_or_default(),
                "created_at": opp.created_at.to_rfc3339(),
                "expires_at": opp.expires_at.map(|dt| dt.to_rfc3339()),
            })
        })
        .collect()
}

pub async fn handle_opportunity_rank(args: Value, state: &AppState) -> Result<String> {
    with_trace(
        &state.trace_manager,
        "mcp.opportunity_rank",
        None,
        None,
        async { inner_handle_opportunity_rank(args, state).await },
    )
    .await
}

async fn inner_handle_opportunity_rank(args: Value, _state: &AppState) -> Result<String> {
    let limit = parse_limit(&args);
    let filters = RankFilters {
        signal_families: parse_signal_families(&args),
        chain_ids: parse_chain_ids(&args),
        min_alpha_score: parse_min_alpha_score(&args),
        action_mode: parse_action_mode(&args),
    };

    let all_opportunities = get_sample_opportunities();
    let filtered = filter_opportunities(all_opportunities, &filters);

    let mut ranked: Vec<Value> = format_opportunities(filtered);
    ranked.truncate(limit);

    for (i, item) in ranked.iter_mut().enumerate() {
        if let Some(obj) = item.as_object_mut() {
            obj.insert("rank".to_string(), json!(i + 1));
        }
    }

    let response = json!({
        "opportunities": ranked,
        "total_count": ranked.len(),
        "limit": limit,
        "filters_applied": {
            "signal_families": filters.signal_families.iter().map(|s| format!("{:?}", s).to_lowercase()).collect::<Vec<_>>(),
            "chain_ids": filters.chain_ids.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
            "min_alpha_score": filters.min_alpha_score,
            "action_mode": filters.action_mode.as_ref().map(|a| format!("{:?}", a).to_lowercase()),
        }
    });

    Ok(serde_json::to_string_pretty(&response)?)
}

#[derive(Debug, Clone, Default)]
pub struct WalletWatchFilters {
    pub wallets: Vec<String>,
    pub chain_ids: Vec<ChainId>,
    pub min_value_usd: Option<f64>,
}

fn parse_wallet_addresses(args: &Value) -> Vec<String> {
    args.get("wallets")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_min_value_usd(args: &Value) -> Option<f64> {
    args.get("min_value_usd")
        .and_then(|v| v.as_f64())
        .filter(|&v| v > 0.0)
}

fn get_sample_wallet_activity() -> Vec<Value> {
    let now = chrono::Utc::now();
    vec![
        json!({
            "wallet": "0xabc123...def",
            "chain_id": "ethereum",
            "activity_type": "swap",
            "tx_hash": "0xtx1",
            "block_number": 19500000,
            "value_usd": 125000.0,
            "tokens_in": ["USDC", "WETH"],
            "tokens_out": ["WETH", "USDC"],
            "protocol": "uniswap_v3",
            "gas_used": 150000,
            "gas_price_gwei": 35.0,
            "timestamp": (now - chrono::Duration::minutes(5)).to_rfc3339(),
            "reorg_safe": true,
        }),
        json!({
            "wallet": "0xabc123...def",
            "chain_id": "ethereum",
            "activity_type": "transfer",
            "tx_hash": "0xtx2",
            "block_number": 19499999,
            "value_usd": 500000.0,
            "tokens_in": ["USDC"],
            "tokens_out": [],
            "protocol": "ethereum",
            "gas_used": 21000,
            "gas_price_gwei": 40.0,
            "timestamp": (now - chrono::Duration::minutes(15)).to_rfc3339(),
            "reorg_safe": true,
        }),
        json!({
            "wallet": "0xwhale...999",
            "chain_id": "polygon",
            "activity_type": "bridge_out",
            "tx_hash": "0xtx3",
            "block_number": 55000000,
            "value_usd": 2500000.0,
            "tokens_in": [],
            "tokens_out": ["USDC"],
            "protocol": "aave_bridge",
            "gas_used": 250000,
            "gas_price_gwei": 80.0,
            "timestamp": (now - chrono::Duration::minutes(30)).to_rfc3339(),
            "reorg_safe": true,
        }),
    ]
}

fn filter_wallet_activity(activity: Vec<Value>, filters: &WalletWatchFilters) -> Vec<Value> {
    activity
        .into_iter()
        .filter(|item| {
            if !filters.wallets.is_empty() {
                let wallet = item.get("wallet").and_then(|v| v.as_str()).unwrap_or("");
                if !filters
                    .wallets
                    .iter()
                    .any(|w| w.starts_with(&wallet[..wallet.len().min(w.len())]))
                {
                    return false;
                }
            }
            if !filters.chain_ids.is_empty() {
                let chain = item.get("chain_id").and_then(|v| v.as_str()).unwrap_or("");
                if !filters.chain_ids.iter().any(|c| c.0 == chain) {
                    return false;
                }
            }
            if let Some(min_value) = filters.min_value_usd {
                let value = item
                    .get("value_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if value < min_value {
                    return false;
                }
            }
            true
        })
        .collect()
}

pub async fn handle_wallet_watch(args: Value, state: &AppState) -> Result<String> {
    with_trace(
        &state.trace_manager,
        "mcp.wallet_watch",
        None,
        None,
        async { inner_handle_wallet_watch(args, state).await },
    )
    .await
}

async fn inner_handle_wallet_watch(args: Value, _state: &AppState) -> Result<String> {
    let filters = WalletWatchFilters {
        wallets: parse_wallet_addresses(&args),
        chain_ids: parse_chain_ids(&args),
        min_value_usd: parse_min_value_usd(&args),
    };

    let all_activity = get_sample_wallet_activity();
    let filtered = filter_wallet_activity(all_activity, &filters);

    let total_value: f64 = filtered
        .iter()
        .filter_map(|v| v.get("value_usd").and_then(|v| v.as_f64()))
        .sum();

    let unique_wallets: Vec<String> = filtered
        .iter()
        .filter_map(|v| v.get("wallet").and_then(|v| v.as_str()).map(String::from))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let response = json!({
        "activities": filtered,
        "summary": {
            "total_activities": filtered.len(),
            "unique_wallets": unique_wallets.len(),
            "total_value_usd": format!("{:.2}", total_value),
        },
        "filters_applied": {
            "wallets": filters.wallets,
            "chain_ids": filters.chain_ids.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
            "min_value_usd": filters.min_value_usd,
        }
    });

    Ok(serde_json::to_string_pretty(&response)?)
}

#[derive(Debug, Clone, Default)]
pub struct ProtocolMonitorFilters {
    pub protocols: Vec<String>,
    pub chain_ids: Vec<ChainId>,
    pub event_types: Vec<String>,
}

fn parse_protocols(args: &Value) -> Vec<String> {
    args.get("protocols")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_event_types(args: &Value) -> Vec<String> {
    args.get("event_types")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn get_sample_protocol_events() -> Vec<Value> {
    let now = chrono::Utc::now();
    vec![
        json!({
            "protocol": "uniswap_v3",
            "chain_id": "ethereum",
            "event_type": "new_pool",
            "pool_address": "0xnewpool123",
            "token0": "USDC",
            "token1": "WETH",
            "fee_tier": 3000,
            "initial_liquidity_usd": 100000.0,
            "tx_hash": "0xevt1",
            "block_number": 19500010,
            "timestamp": (now - chrono::Duration::minutes(2)).to_rfc3339(),
        }),
        json!({
            "protocol": "aave_v3",
            "chain_id": "ethereum",
            "event_type": "large_liquidation",
            "health_factor": 0.95,
            "liquidated_value_usd": 750000.0,
            "collateral_token": "WBTC",
            "debt_token": "USDC",
            "tx_hash": "0xevt2",
            "block_number": 19500008,
            "timestamp": (now - chrono::Duration::minutes(8)).to_rfc3339(),
        }),
        json!({
            "protocol": "curve",
            "chain_id": "ethereum",
            "event_type": "parameter_change",
            "parameter": " amplification_coeff",
            "old_value": "100",
            "new_value": "150",
            "tx_hash": "0xevt3",
            "block_number": 19500005,
            "timestamp": (now - chrono::Duration::minutes(20)).to_rfc3339(),
        }),
        json!({
            "protocol": "compound_v3",
            "chain_id": "polygon",
            "event_type": "market_listing",
            "token": "WMATIC",
            "collateral_factor": 0.75,
            "liquidation_threshold": 0.80,
            "tx_hash": "0xevt4",
            "block_number": 55000100,
            "timestamp": (now - chrono::Duration::minutes(45)).to_rfc3339(),
        }),
        json!({
            "protocol": "uniswap_v3",
            "chain_id": "arbitrum",
            "event_type": "fee_switch",
            "pool_address": "0xpool456",
            "token0": "USDC",
            "token1": "USDCe",
            "old_fee": 500,
            "new_fee": 3000,
            "tx_hash": "0xevt5",
            "block_number": 120000000,
            "timestamp": (now - chrono::Duration::hours(1)).to_rfc3339(),
        }),
    ]
}

fn filter_protocol_events(events: Vec<Value>, filters: &ProtocolMonitorFilters) -> Vec<Value> {
    events
        .into_iter()
        .filter(|item| {
            if !filters.protocols.is_empty() {
                let protocol = item.get("protocol").and_then(|v| v.as_str()).unwrap_or("");
                if !filters
                    .protocols
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case(protocol))
                {
                    return false;
                }
            }
            if !filters.chain_ids.is_empty() {
                let chain = item.get("chain_id").and_then(|v| v.as_str()).unwrap_or("");
                if !filters.chain_ids.iter().any(|c| c.0 == chain) {
                    return false;
                }
            }
            if !filters.event_types.is_empty() {
                let event_type = item
                    .get("event_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !filters
                    .event_types
                    .iter()
                    .any(|e| e.eq_ignore_ascii_case(event_type))
                {
                    return false;
                }
            }
            true
        })
        .collect()
}

pub async fn handle_protocol_monitor(args: Value, state: &AppState) -> Result<String> {
    with_trace(
        &state.trace_manager,
        "mcp.protocol_monitor",
        None,
        None,
        async { inner_handle_protocol_monitor(args, state).await },
    )
    .await
}

async fn inner_handle_protocol_monitor(args: Value, _state: &AppState) -> Result<String> {
    let filters = ProtocolMonitorFilters {
        protocols: parse_protocols(&args),
        chain_ids: parse_chain_ids(&args),
        event_types: parse_event_types(&args),
    };

    let all_events = get_sample_protocol_events();
    let filtered = filter_protocol_events(all_events, &filters);

    let mut events_by_protocol: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for event in &filtered {
        if let Some(protocol) = event.get("protocol").and_then(|v| v.as_str()) {
            *events_by_protocol.entry(protocol.to_string()).or_insert(0) += 1;
        }
    }

    let mut events_by_type: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for event in &filtered {
        if let Some(event_type) = event.get("event_type").and_then(|v| v.as_str()) {
            *events_by_type.entry(event_type.to_string()).or_insert(0) += 1;
        }
    }

    let response = json!({
        "events": filtered,
        "summary": {
            "total_events": filtered.len(),
            "events_by_protocol": events_by_protocol,
            "events_by_type": events_by_type,
        },
        "filters_applied": {
            "protocols": filters.protocols,
            "chain_ids": filters.chain_ids.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
            "event_types": filters.event_types,
        }
    });

    Ok(serde_json::to_string_pretty(&response)?)
}
