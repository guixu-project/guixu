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

use crate::discovery::runtime::DataDiscoveryRuntime;
use crate::host_sampling::HostSamplingRuntime;

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
    /// Agent trace manager (None if tracing is disabled).
    pub trace_manager:
        Option<Arc<tokio::sync::RwLock<data_storage::trace_manager::AgentTraceManager>>>,
    pub search_workers: usize,
    pub discovery_runtime: Option<Arc<DataDiscoveryRuntime>>,
    pub sampling_runtime: Option<Arc<HostSamplingRuntime>>,
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
        Self::for_codex_with_search_workers(0).await
    }

    pub async fn for_codex_with_search_workers(search_workers: usize) -> Result<Self> {
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
        .await
        .with_search_workers(search_workers))
    }

    pub async fn for_local_store(
        identity: NodeIdentity,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        payment: &PaymentConfig,
    ) -> Self {
        Self::for_local_store_with_search_workers(identity, store, feedback_store, payment, 0).await
    }

    pub async fn for_local_store_with_search_workers(
        identity: NodeIdentity,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        payment: &PaymentConfig,
        search_workers: usize,
    ) -> Self {
        let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::channel(8);
        let dht = DhtIndex::new(NetworkHandle {
            cmd_tx,
            local_peer_id: libp2p::PeerId::random(),
        });

        let job_store = JobStore::open(&NodeConfig::config_dir().join("jobs"))
            .expect("failed to open job store");

        Self::with_payment_config_with_search_workers(
            identity,
            dht,
            store,
            feedback_store,
            job_store,
            payment,
            search_workers,
        )
        .await
    }

    pub async fn with_payment_config(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        job_store: JobStore,
        payment: &PaymentConfig,
    ) -> Self {
        Self::with_payment_config_with_search_workers(
            identity,
            dht,
            store,
            feedback_store,
            job_store,
            payment,
            0,
        )
        .await
    }

    pub async fn with_payment_config_with_search_workers(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        job_store: JobStore,
        payment: &PaymentConfig,
        search_workers: usize,
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
        .with_search_workers(search_workers)
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
        Self::with_full_config_with_search_workers(
            identity,
            dht,
            store,
            feedback_store,
            job_store,
            payment,
            duckdb_catalogs,
            pg_catalogs,
            sql_catalogs,
            0,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn with_full_config_with_search_workers(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        job_store: JobStore,
        payment: &PaymentConfig,
        duckdb_catalogs: &[DuckDbCatalog],
        pg_catalogs: &[PostgreSqlCatalog],
        sql_catalogs: &[SqlEndpointCatalog],
        search_workers: usize,
    ) -> Self {
        let vector_index = VectorIndex;
        let intent_parser = IntentParser;
        let adapters = adapters_with_config(&[], duckdb_catalogs, pg_catalogs, sql_catalogs);
        let search_engine = SearchEngine::new(vector_index, intent_parser, adapters);
        let search_engine = Arc::new(search_engine);

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

        let mut state = Self {
            identity,
            dht,
            store,
            feedback_store,
            job_store,
            tcv_engine: TcvEngine,
            search_engine,
            payment_router: PaymentRouter::new(wallet, payment.testnet),
            torrent_engine,
            trace_manager: None,
            search_workers: 0,
            discovery_runtime: None,
            sampling_runtime: None,
        };
        state.configure_discovery_runtime(search_workers);
        state
    }

    pub fn with_sampling_runtime(mut self, sampling_runtime: Arc<HostSamplingRuntime>) -> Self {
        self.sampling_runtime = Some(sampling_runtime);
        if self.search_workers > 0 {
            let search_workers = self.search_workers;
            self.configure_discovery_runtime(search_workers);
        }
        self
    }

    fn configure_discovery_runtime(&mut self, search_workers: usize) {
        self.search_workers = search_workers;
        if search_workers == 0 {
            self.discovery_runtime = None;
            return;
        }

        self.discovery_runtime = match DataDiscoveryRuntime::try_new(
            search_workers,
            self.sampling_runtime.clone(),
            self.search_engine.clone(),
            self.feedback_store.clone(),
            self.store.clone(),
        ) {
            Ok(runtime) => Some(Arc::new(runtime)),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    search_workers,
                    "discovery runtime init failed; agentic dataset_search remains required"
                );
                None
            }
        };
    }

    fn with_search_workers(mut self, search_workers: usize) -> Self {
        self.configure_discovery_runtime(search_workers);
        self
    }
}
