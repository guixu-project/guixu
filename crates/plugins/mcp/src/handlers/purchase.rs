// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use chrono::Utc;
use data_core::types::{ArtifactRef, DeliveryManifest, IngestJob, IngestState};
use data_trading::router::TransactionContext;
use serde_json::json;

use crate::state::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid_str = args.get("cid").and_then(|v| v.as_str()).unwrap_or("");
    let max_price = args
        .get("max_price")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let cid = data_core::types::DatasetCid(cid_str.to_string());
    let metadata = match state.store.get(&cid)? {
        Some(m) => m,
        None => anyhow::bail!("Dataset {cid_str} not found"),
    };

    if metadata.price.amount > max_price && max_price > 0.0 {
        anyhow::bail!(
            "Price ${:.2} exceeds budget ${:.2}",
            metadata.price.amount,
            max_price
        );
    }

    let tx_ctx = TransactionContext {
        buyer: state.identity.did.clone(),
        seller: metadata.provider.clone(),
        dataset_cid: cid.clone(),
        amount: metadata.price.amount,
        is_single_request: true,
        is_session_batch: false,
        prefer_fiat: false,
        requires_verification: metadata.price.amount > 1.0,
        seller_endpoint: args
            .get("seller_endpoint")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                if let Some(listing_id) = cid_str.strip_prefix("guixu-hub:") {
                    let base = std::env::var("GUIXU_HUB_BASE_URL")
                        .unwrap_or_else(|_| "https://www.guixu.org".into());
                    Some(format!("{base}/api/x402/{listing_id}"))
                } else {
                    None
                }
            }),
        seller_headers: {
            let mut hdrs: Vec<(String, String)> = args
                .get("seller_headers")
                .and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(rid) = args.get("valuation_report_id").and_then(|v| v.as_str()) {
                hdrs.push(("X-Valuation-Report-Id".into(), rid.into()));
            }
            if let Some(rd) = args.get("valuation_report_digest").and_then(|v| v.as_str()) {
                hdrs.push(("X-Valuation-Report-Digest".into(), rd.into()));
            }
            if let Some(vm) = args.get("valuation_mode").and_then(|v| v.as_str()) {
                hdrs.push(("X-Valuation-Mode".into(), vm.into()));
            }
            if hdrs.is_empty() {
                None
            } else {
                Some(hdrs)
            }
        },
    };

    let (receipt, protocol_desc) = if metadata.price.is_free() {
        (None, "Free dataset — no payment required")
    } else {
        let protocol = state.payment_router.select_protocol(&tx_ctx);
        let r = state.payment_router.pay(protocol, &tx_ctx).await?;
        let desc = match protocol {
            data_core::types::PaymentProtocol::X402 => "Micropayment via x402 (USDC on Base L2)",
            data_core::types::PaymentProtocol::StripeMpp => "Session payment via Stripe MPP",
            data_core::types::PaymentProtocol::Erc8183 => {
                "Escrowed payment via ERC-8183 (verify then release)"
            }
        };
        (Some(r), desc)
    };

    let tx_id = receipt
        .as_ref()
        .map(|r| r.tx_id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let protocol_name = receipt
        .as_ref()
        .map(|r| format!("{:?}", r.protocol))
        .unwrap_or_else(|| "none".into());

    let delivery = if let Some(download_url) = receipt
        .as_ref()
        .and_then(|r| r.seller_response.as_ref())
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|v| v.get("downloadUrl")?.as_str().map(String::from))
    {
        ("url".to_string(), download_url, None)
    } else {
        match state.store.get_file_path(&cid)? {
            Some(path) if path.exists() => {
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                (
                    "local".to_string(),
                    path.to_string_lossy().to_string(),
                    Some(size),
                )
            }
            _ => {
                let download_dir = state
                    .store
                    .get_file_path(&data_core::types::DatasetCid("__config_data_dir__".into()))?
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp/guixu-downloads"));
                let dest = download_dir.join(format!("{}.dat", &cid_str[..16.min(cid_str.len())]));
                (
                    "torrent_pending".to_string(),
                    dest.to_string_lossy().to_string(),
                    None,
                )
            }
        }
    };

    let order_id = format!("order_{}", tx_id);
    let delivery_id = format!("delivery_{}", uuid::Uuid::new_v4());
    let target_bytes = delivery.2.unwrap_or(0);
    let delivery_type = delivery.0.clone();
    let delivery_uri = delivery.1.clone();

    let manifest = DeliveryManifest {
        order_id: order_id.clone(),
        delivery_id: delivery_id.clone(),
        dataset_id: cid_str.to_string(),
        artifacts: vec![ArtifactRef {
            artifact_id: format!("artifact_{}", uuid::Uuid::new_v4()),
            protocol: delivery_type.clone(),
            uri: delivery_uri.clone(),
            size_bytes: target_bytes,
            checksum: None,
            supports_range: true,
            required_headers: None,
        }],
        packaging: Some(data_core::types::PackagingInfo {
            format: Some("zip".to_string()),
            compression: Some("deflate".to_string()),
        }),
        access: None,
    };

    let job = IngestJob {
        job_id: uuid::Uuid::new_v4(),
        order_id: Some(order_id.clone()),
        dataset_id: cid_str.to_string(),
        manifest: manifest.clone(),
        state: IngestState::Pending,
        target_bytes,
        downloaded_bytes: 0,
        verified_bytes: 0,
        resume_token: None,
        failure_reason: None,
        started_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
    };

    state.job_store.put_ingest_job(&job)?;

    Ok(json!({
        "status": "purchased",
        "cid": cid_str,
        "price_paid": metadata.price.amount,
        "payment_protocol": protocol_name,
        "protocol_description": protocol_desc,
        "tx_id": tx_id,
        "on_chain_receipt": "EAS attestation simulated (Base L2)",
        "delivery": {
            "method": delivery_type,
            "download_url": None::<String>,
            "file_path": None::<String>,
            "download_path": None::<String>,
            "info_hash": metadata.info_hash,
        },
        "manifest": manifest,
        "ingest_job_id": job.job_id.to_string(),
    })
    .to_string())
}
