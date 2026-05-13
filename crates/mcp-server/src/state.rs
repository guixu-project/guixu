// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) 2026 The State Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::config::{FeaturesConfig, NodeConfig, PaymentConfig};
use data_core::identity::NodeIdentity;
use data_core::signal_fetcher::SignalFetcher;
use data_p2p::dht::DhtIndex;
use data_p2p::p2p_handle::P2PHandle;
use data_search::adapters::default_adapters_filtered;
use data_search::engine::SearchEngine;
use data_search::intent::IntentParser;
use data_search::vector_index::VectorIndex;
use data_storage::feedback_store::FeedbackStore;
use data_storage::metadata_store::MetadataStore;
use data_trading::router::{PaymentMode, PaymentRouter};
use data_trading::wallet::AgentWallet;
use data_valuation::tcv::TcvEngine;
use std::sync::Arc;

/// Shared state accessible by MCP tool handlers.
pub struct AppState {
    pub identity: NodeIdentity,
    pub dht: DhtIndex,
    pub store: MetadataStore,
    pub feedback_store: FeedbackStore,
    pub tcv_engine: TcvEngine,
    pub search_engine: SearchEngine,
    pub payment_router: PaymentRouter,
    /// GIP005: Signal fetcher based on features.on_chain_signal.
    pub signal_fetcher: SignalFetcher,
    /// Lazy-initialized P2P subsystem handle.
    pub p2p_handle: P2PHandle,
    /// Feature flags for UI/cli guidance.
    pub features: FeaturesConfig,
}

impl AppState {
    pub async fn new(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
    ) -> Self {
        Self::with_payment_config(
            identity,
            dht,
            store,
            feedback_store,
            &PaymentConfig::default(),
            &[],
            &FeaturesConfig::default(),
        )
        .await
    }

    /// Create AppState with full feature flags.
    pub async fn with_features(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        features: &FeaturesConfig,
    ) -> Self {
        Self::with_payment_config(
            identity,
            dht,
            store,
            feedback_store,
            &PaymentConfig::default(),
            &[],
            features,
        )
        .await
    }

    pub async fn with_payment_config(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        payment: &PaymentConfig,
        disabled_adapters: &[String],
        features: &FeaturesConfig,
    ) -> Self {
        let vector_index = VectorIndex;
        let intent_parser = IntentParser::default();
        let adapters = default_adapters_filtered(disabled_adapters);
        let search_engine = SearchEngine::new(vector_index, intent_parser, adapters);

        // GIP005: Respect features.blockchain_payment — no hardcoded fallback
        let payment_router = if features.blockchain_payment {
            match AgentWallet::from_keyfile(&payment.wallet_key_path) {
                Ok(wallet) => PaymentRouter::new(PaymentMode::Enabled {
                    wallet: Box::new(wallet),
                    testnet: payment.testnet,
                    facilitator_url: payment.facilitator_url.clone(),
                }),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "wallet key not found — falling back to disabled payments"
                    );
                    PaymentRouter::new(PaymentMode::Disabled)
                }
            }
        } else {
            tracing::info!("blockchain_payment disabled — payments unavailable");
            PaymentRouter::new(PaymentMode::Disabled)
        };

        // GIP005: P2P lazy initialization via P2PHandle
        let download_dir = NodeConfig::config_dir().join("downloads");
        let p2p_handle = P2PHandle::new(features.p2p_torrent, download_dir);

        // GIP005: TcvEngine takes configurable weights
        let tcv_engine = TcvEngine::new(features.tcv_weights.clone());

        // GIP005: Build signal_fetcher based on features.on_chain_signal
        let signal_fetcher = if features.on_chain_signal {
            SignalFetcher::local_only(Arc::new(feedback_store.clone()))
        } else {
            SignalFetcher::no_op()
        };

        Self {
            identity,
            dht,
            store,
            feedback_store,
            tcv_engine,
            search_engine,
            payment_router,
            signal_fetcher,
            p2p_handle,
            features: features.clone(),
        }
    }
}