use data_core::config::PaymentConfig;
use data_core::identity::NodeIdentity;
use data_p2p::dht::DhtIndex;
use data_p2p::torrent::TorrentEngine;
use data_search::adapters::default_adapters;
use data_search::engine::SearchEngine;
use data_search::intent::IntentParser;
use data_search::vector_index::VectorIndex;
use data_storage::feedback_store::FeedbackStore;
use data_storage::metadata_store::MetadataStore;
use data_trading::router::PaymentRouter;
use data_trading::wallet::AgentWallet;
use data_valuation::tcv::TcvEngine;

/// Shared state accessible by MCP tool handlers.
pub struct AppState {
    pub identity: NodeIdentity,
    pub dht: DhtIndex,
    pub store: MetadataStore,
    pub feedback_store: FeedbackStore,
    pub tcv_engine: TcvEngine,
    pub search_engine: SearchEngine,
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
        Self::with_payment_config(
            identity,
            dht,
            store,
            feedback_store,
            &PaymentConfig::default(),
        )
        .await
    }

    pub async fn with_payment_config(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
        payment: &PaymentConfig,
    ) -> Self {
        let vector_index = VectorIndex;
        let intent_parser = IntentParser::default();
        let adapters = default_adapters();
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

        let download_dir = data_core::config::NodeConfig::config_dir().join("downloads");
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
            tcv_engine: TcvEngine,
            search_engine,
            payment_router: PaymentRouter::new(wallet, payment.testnet),
            torrent_engine,
        }
    }
}
