// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::config::{
    DuckDbCatalog, NodeConfig, PaymentConfig, PostgreSqlCatalog, SqlEndpointCatalog,
};
use data_core::identity::NodeIdentity;
use data_p2p::dht::DhtIndex;
use data_p2p::network::NetworkHandle;
use data_p2p::torrent::TorrentEngine;
use data_search::adapters::adapters_with_config;
use data_search::engine::SearchEngine;
use data_search::intent::IntentParser;
use data_search::vector_index::VectorIndex;
use data_storage::feedback_store::FeedbackStore;
use data_storage::job_store::JobStore;
use data_storage::metadata_store::MetadataStore;
use data_trading::router::PaymentRouter;
use data_trading::wallet::AgentWallet;
use data_valuation::tcv::TcvEngine;
use std::sync::Arc;

/// Shared state accessible by MCP tool handlers.
pub struct AppState {
    pub identity: NodeIdentity,
    pub dht: DhtIndex,
    pub store: MetadataStore,
    pub feedback_store: FeedbackStore,
    pub job_store: JobStore,
    pub tcv_engine: TcvEngine,
    pub search_engine: Arc<SearchEngine>,
    pub payment_router: PaymentRouter,
    pub torrent_engine: Option<TorrentEngine>,
}

impl AppState {
    pub async fn new(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
    ) -> Self {
        let job_store = JobStore::open(&NodeConfig::config_dir().join("jobs"))
            .expect("failed to open job store");
        Self::with_payment_config(
            identity,
            dht,
            store,
            feedback_store,
            job_store,
            &PaymentConfig::default(),
        )
        .await
    }

    pub async fn for_codex() -> Result<Self> {
        let identity = NodeIdentity::generate();
        let session_suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let base_dir = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("local")
            .join("codex-mcp")
            .join("sessions")
            .join(format!("{}-{session_suffix}", std::process::id()));
        std::fs::create_dir_all(&base_dir)?;

        let store = MetadataStore::open(&base_dir.join("db"))?;
        let feedback_store = FeedbackStore::open(&base_dir.join("feedback_db"))?;
        let job_store = JobStore::open(&base_dir.join("job_db"))?;

        let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::channel(8);
        let dht = DhtIndex::new(NetworkHandle {
            cmd_tx,
            local_peer_id: libp2p::PeerId::random(),
        });

        Ok(Self::with_payment_config(
            identity,
            dht,
            store,
            feedback_store,
            job_store,
            &PaymentConfig::default(),
        )
        .await)
    }

    pub async fn for_local_store(
        identity: NodeIdentity,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        payment: &PaymentConfig,
    ) -> Self {
        let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::channel(8);
        let dht = DhtIndex::new(NetworkHandle {
            cmd_tx,
            local_peer_id: libp2p::PeerId::random(),
        });

        let job_store = JobStore::open(&NodeConfig::config_dir().join("jobs"))
            .expect("failed to open job store");

        Self::with_payment_config(identity, dht, store, feedback_store, job_store, payment).await
    }

    pub async fn with_payment_config(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        job_store: JobStore,
        payment: &PaymentConfig,
    ) -> Self {
        Self::with_full_config(
            identity,
            dht,
            store,
            feedback_store,
            job_store,
            payment,
            &[],
            &[],
            &[],
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn with_full_config(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        job_store: JobStore,
        payment: &PaymentConfig,
        duckdb_catalogs: &[DuckDbCatalog],
        pg_catalogs: &[PostgreSqlCatalog],
        sql_catalogs: &[SqlEndpointCatalog],
    ) -> Self {
        let vector_index = VectorIndex;
        let intent_parser = IntentParser;
        let adapters = adapters_with_config(&[], duckdb_catalogs, pg_catalogs, sql_catalogs);
        let search_engine = SearchEngine::new(vector_index, intent_parser, adapters);

        let wallet = AgentWallet::from_keyfile(&payment.wallet_key_path).unwrap_or_else(|_| {
            tracing::warn!(
                "No wallet key at {} — payments will fail.",
                payment.wallet_key_path.display()
            );
            AgentWallet::from_private_key(
                "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            )
            .expect("hardcoded key")
        });

        let download_dir = NodeConfig::config_dir().join("downloads");
        let torrent_engine = match TorrentEngine::new(download_dir).await {
            Ok(engine) => {
                tracing::info!("torrent engine initialized");
                Some(engine)
            }
            Err(e) => {
                tracing::warn!(error = %e, "torrent engine init failed — BT downloads disabled");
                None
            }
        };

        Self {
            identity,
            dht,
            store,
            feedback_store,
            job_store,
            tcv_engine: TcvEngine,
            search_engine: Arc::new(search_engine),
            payment_router: PaymentRouter::new(wallet, payment.testnet),
            torrent_engine,
        }
    }
}
