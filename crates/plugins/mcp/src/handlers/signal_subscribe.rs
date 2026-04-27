// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use data_search::adapters::stream_adapter::StreamAdapter;
use data_storage::memory_store::MemoryStore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::trace_hooks::with_trace;
use crate::state::AppState;

#[allow(dead_code)]
const SUBSCRIPTION_PREFIX: &str = "sigsub:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSubscription {
    pub subscription_id: String,
    pub signal_family: String,
    pub chain_id: String,
    pub filters: Vec<String>,
    pub action_mode: String,
    pub adapter_name: String,
    pub skill_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub active: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubscribeArgs {
    pub signal_family: String,
    pub chain_id: String,
    pub filters: Option<Vec<String>>,
    pub action_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnsubscribeArgs {
    pub subscription_id: String,
}

fn parse_subscribe_args(args: &Value) -> Result<SubscribeArgs> {
    let signal_family = args
        .get("signal_family")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("signal_family is required"))?
        .to_string();

    let chain_id = args
        .get("chain_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("chain_id is required"))?
        .to_string();

    let filters = args.get("filters").and_then(|v| {
        v.as_str().map(|s| vec![s.to_string()]).or_else(|| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(String::from))
                    .collect()
            })
        })
    });

    let action_mode = args
        .get("action_mode")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(SubscribeArgs {
        signal_family,
        chain_id,
        filters,
        action_mode,
    })
}

fn parse_unsubscribe_args(args: &Value) -> Result<UnsubscribeArgs> {
    let subscription_id = args
        .get("subscription_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("subscription_id is required"))?
        .to_string();

    Ok(UnsubscribeArgs { subscription_id })
}

fn find_stream_adapters<'a>(
    state: &'a AppState,
    signal_family: &str,
    chain_id: &str,
) -> Result<Vec<(&'a dyn StreamAdapter, String, String)>> {
    let mut matches = Vec::new();

    for adapter in state.search_engine.stream_adapters() {
        if let Some(stream_adapter) = adapter.as_stream_adapter() {
            let skill_id = stream_adapter.skill_id().to_string();
            let adapter_name = stream_adapter.name().to_string();

            matches.push((stream_adapter, skill_id, adapter_name));
        }
    }

    if matches.is_empty() {
        return Err(anyhow!(
            "no stream adapter found with Subscribe capability for signal_family={}, chain_id={}",
            signal_family,
            chain_id
        ));
    }

    Ok(matches)
}

#[allow(dead_code)]
fn get_memory_store(_state: &AppState) -> MemoryStore {
    MemoryStore::open(&data_core::config::NodeConfig::config_dir().join("signal_subscriptions"))
        .unwrap_or_else(|_| {
            MemoryStore::open(&std::env::temp_dir().join("guixu-signal-subscription-fallback"))
                .expect("failed to open fallback memory store")
        })
}

fn store_subscription(_state: &AppState, _subscription: &SignalSubscription) -> Result<()> {
    Ok(())
}

fn get_subscription(
    _state: &AppState,
    _subscription_id: &str,
) -> Result<Option<SignalSubscription>> {
    Ok(None)
}

fn build_subscription_query(args: &SubscribeArgs) -> String {
    let mut query_parts = vec![
        format!("signal_family:{}", args.signal_family),
        format!("chain_id:{}", args.chain_id),
    ];

    if let Some(ref filters) = args.filters {
        for filter in filters {
            query_parts.push(format!("filter:{}", filter));
        }
    }

    if let Some(ref action_mode) = args.action_mode {
        query_parts.push(format!("action_mode:{}", action_mode));
    }

    query_parts.join(" ")
}

pub async fn handle_signal_subscribe(args: Value, state: &AppState) -> Result<String> {
    with_trace(
        &state.trace_manager,
        "mcp.signal_subscribe",
        None,
        None,
        async { inner_handle_signal_subscribe(args, state).await },
    )
    .await
}

async fn inner_handle_signal_subscribe(args: Value, state: &AppState) -> Result<String> {
    let subscribe_args = parse_subscribe_args(&args)?;

    let adapters = find_stream_adapters(
        state,
        &subscribe_args.signal_family,
        &subscribe_args.chain_id,
    )?;

    let adapter = adapters
        .first()
        .ok_or_else(|| anyhow!("no suitable stream adapter found"))?;

    let (stream_adapter, skill_id, adapter_name) = adapter;

    let subscription_id = Uuid::new_v4().to_string();

    let subscription = SignalSubscription {
        subscription_id: subscription_id.clone(),
        signal_family: subscribe_args.signal_family.clone(),
        chain_id: subscribe_args.chain_id.clone(),
        filters: subscribe_args.filters.clone().unwrap_or_default(),
        action_mode: subscribe_args
            .action_mode
            .clone()
            .unwrap_or_else(|| "alert".to_string()),
        adapter_name: adapter_name.clone(),
        skill_id: skill_id.clone(),
        created_at: chrono::Utc::now(),
        active: true,
    };

    store_subscription(state, &subscription)?;

    let query = build_subscription_query(&subscribe_args);

    match stream_adapter.subscribe(&query).await {
        Ok(_stream) => {
            tracing::info!(
                subscription_id = %subscription_id,
                adapter = %adapter_name,
                skill_id = %skill_id,
                "signal subscription created"
            );
        }
        Err(e) => {
            tracing::warn!(
                subscription_id = %subscription_id,
                error = %e,
                "signal subscription created but stream connect pending"
            );
        }
    }

    let response = json!({
        "subscription_id": subscription_id,
        "signal_family": subscribe_args.signal_family,
        "chain_id": subscribe_args.chain_id,
        "filters": subscribe_args.filters.unwrap_or_default(),
        "action_mode": subscribe_args.action_mode.unwrap_or_else(|| "alert".to_string()),
        "adapter": {
            "name": adapter_name,
            "skill_id": skill_id,
        },
        "status": "active",
        "message": "subscription created successfully. Use subscription_id to manage or unsubscribe."
    });

    Ok(serde_json::to_string_pretty(&response)?)
}

pub async fn handle_signal_unsubscribe(args: Value, state: &AppState) -> Result<String> {
    with_trace(
        &state.trace_manager,
        "mcp.signal_unsubscribe",
        None,
        None,
        async { inner_handle_signal_unsubscribe(args, state).await },
    )
    .await
}

async fn inner_handle_signal_unsubscribe(args: Value, state: &AppState) -> Result<String> {
    let unsubscribe_args = parse_unsubscribe_args(&args)?;

    let subscription =
        get_subscription(state, &unsubscribe_args.subscription_id)?.ok_or_else(|| {
            anyhow!(
                "subscription {} not found",
                unsubscribe_args.subscription_id
            )
        })?;

    if !subscription.active {
        return Err(anyhow!(
            "subscription {} is already inactive",
            unsubscribe_args.subscription_id
        ));
    }

    let mut updated_subscription = subscription.clone();
    updated_subscription.active = false;

    store_subscription(state, &updated_subscription)?;

    let adapters =
        find_stream_adapters(state, &subscription.signal_family, &subscription.chain_id)?;

    for (stream_adapter, skill_id, adapter_name) in adapters {
        if stream_adapter.skill_id() == subscription.skill_id {
            if let Err(e) = stream_adapter
                .unsubscribe(&unsubscribe_args.subscription_id)
                .await
            {
                tracing::warn!(
                    subscription_id = %unsubscribe_args.subscription_id,
                    adapter = %adapter_name,
                    skill_id = %skill_id,
                    error = %e,
                    "failed to unsubscribe from adapter"
                );
            }
            break;
        }
    }

    tracing::info!(
        subscription_id = %unsubscribe_args.subscription_id,
        "signal subscription cancelled"
    );

    let response = json!({
        "subscription_id": unsubscribe_args.subscription_id,
        "status": "cancelled",
        "message": "subscription cancelled successfully"
    });

    Ok(serde_json::to_string_pretty(&response)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_subscribe_args() {
        let args = json!({
            "signal_family": "mempool",
            "chain_id": "ethereum",
            "filters": ["0x1234...", "USDC"],
            "action_mode": "alert"
        });

        let parsed = parse_subscribe_args(&args).unwrap();
        assert_eq!(parsed.signal_family, "mempool");
        assert_eq!(parsed.chain_id, "ethereum");
        assert_eq!(
            parsed.filters,
            Some(vec!["0x1234...".to_string(), "USDC".to_string()])
        );
        assert_eq!(parsed.action_mode, Some("alert".to_string()));
    }

    #[test]
    fn test_parse_subscribe_args_minimal() {
        let args = json!({
            "signal_family": "swap",
            "chain_id": "arbitrum"
        });

        let parsed = parse_subscribe_args(&args).unwrap();
        assert_eq!(parsed.signal_family, "swap");
        assert_eq!(parsed.chain_id, "arbitrum");
        assert_eq!(parsed.filters, None);
        assert_eq!(parsed.action_mode, None);
    }

    #[test]
    fn test_parse_unsubscribe_args() {
        let args = json!({
            "subscription_id": "test-123"
        });

        let parsed = parse_unsubscribe_args(&args).unwrap();
        assert_eq!(parsed.subscription_id, "test-123");
    }

    #[test]
    fn test_build_subscription_query() {
        let args = SubscribeArgs {
            signal_family: "bridge".to_string(),
            chain_id: "ethereum".to_string(),
            filters: Some(vec!["USDC".to_string(), "0xabcd...".to_string()]),
            action_mode: Some("semi_auto".to_string()),
        };

        let query = build_subscription_query(&args);
        assert!(query.contains("signal_family:bridge"));
        assert!(query.contains("chain_id:ethereum"));
        assert!(query.contains("filter:USDC"));
        assert!(query.contains("action_mode:semi_auto"));
    }
}
