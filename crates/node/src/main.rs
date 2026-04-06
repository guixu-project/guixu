// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use data_core::config::{NodeConfig, NodeMode};
use data_core::env::load_local_settings;
use data_core::identity::NodeIdentity;
use data_core::types::AccessMode;
use data_mcp_server::server::{AppState, McpServer};
use data_p2p::dht::DhtIndex;
use data_storage::feedback_store::FeedbackStore;
use data_storage::metadata_store::MetadataStore;

mod mcp_install;

#[derive(Parser)]
#[command(
    name = "data-node",
    about = "Guixu: On-Chain Data Valuation for AI Agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new node (generate identity + config).
    Init {
        #[arg(long)]
        data_dir: Option<String>,
    },
    /// Start the full node (P2P + auto-publish + MCP server).
    Start,
    /// Run as MCP server only (for Agent integration).
    Mcp {
        #[command(subcommand)]
        action: Option<McpAction>,

        #[arg(long, default_value = "light", global = true)]
        mode: String,
    },
}

#[derive(Subcommand)]
enum McpAction {
    /// Register Guixu MCP with an AI client.
    Install {
        /// Target client: codex, cursor, claude-code, opencode, openclaw
        client: Option<String>,
    },
    /// Remove Guixu MCP from an AI client.
    Uninstall {
        /// Target client: codex, cursor, claude-code, opencode, openclaw
        client: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(error) = load_local_settings() {
        eprintln!("warning: failed to load local/settings.env: {error}");
    }

    let cli = Cli::parse();
    init_logging(&cli.command);

    match cli.command {
        Commands::Init { data_dir } => cmd_init(data_dir)?,
        Commands::Start => cmd_start().await?,
        Commands::Mcp { action, mode } => match action {
            Some(McpAction::Install { client }) => cmd_mcp_install(client)?,
            Some(McpAction::Uninstall { client }) => cmd_mcp_uninstall(&client)?,
            None => cmd_mcp(mode).await?,
        },
    }

    Ok(())
}

fn init_logging(command: &Commands) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let use_stderr_only = matches!(command, Commands::Mcp { mode, .. } if mode == "codex");
    if use_stderr_only {
        init_stderr_logging(env_filter);
        return;
    }

    let log_dir = NodeConfig::config_dir().join("logs");
    let file_appender = std::fs::create_dir_all(&log_dir)
        .map_err(anyhow::Error::from)
        .and_then(|_| {
            tracing_appender::rolling::RollingFileAppender::builder()
                .rotation(tracing_appender::rolling::Rotation::DAILY)
                .filename_prefix("guixu.log")
                .build(&log_dir)
                .map_err(anyhow::Error::from)
        });

    match file_appender {
        Ok(file_appender) => {
            let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
            tracing_subscriber::registry()
                .with(
                    fmt::layer()
                        .with_writer(std::io::stderr)
                        .with_target(false)
                        .compact(),
                )
                .with(
                    fmt::layer()
                        .with_writer(file_writer)
                        .json()
                        .with_span_list(false),
                )
                .with(env_filter)
                .init();
        }
        Err(e) => {
            eprintln!(
                "warning: file logging unavailable at {}, falling back to stderr-only logging: {e}",
                log_dir.display()
            );
            init_stderr_logging(env_filter);
        }
    }
}

fn init_stderr_logging(env_filter: EnvFilter) {
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false)
                .compact(),
        )
        .with(env_filter)
        .init();
}

fn cmd_init(data_dir: Option<String>) -> Result<()> {
    let config_dir = NodeConfig::config_dir();
    std::fs::create_dir_all(&config_dir)?;

    let identity = NodeIdentity::generate();
    std::fs::write(NodeConfig::identity_path(), identity.seed())?;
    info!(did = %identity.did.0, "generated node identity");

    let mut config = NodeConfig::default();
    if let Some(dir) = data_dir {
        config.data_dir = shellexpand(dir);
    }
    std::fs::create_dir_all(&config.data_dir)?;
    let toml_str = toml::to_string_pretty(&config)?;
    std::fs::write(NodeConfig::config_path(), &toml_str)?;

    println!("✅ Node initialized");
    println!("   DID:       {}", identity.did.0);
    println!("   Config:    {}", NodeConfig::config_path().display());
    println!("   Data dir:  {}", config.data_dir.display());
    Ok(())
}

async fn cmd_start() -> Result<()> {
    let (config, identity) = load_config_and_identity()?;
    info!(did = %identity.did.0, privacy = ?config.privacy_level, "starting full node");

    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let feedback_store = FeedbackStore::open(&NodeConfig::config_dir().join("feedback_db"))?;

    // Build privacy config from node config
    let privacy_config = data_auth::privacy::PrivacyConfig {
        level: match config.privacy_level {
            data_core::config::PrivacyLevel::Off => data_auth::privacy::PrivacyLevel::Off,
            data_core::config::PrivacyLevel::Standard => data_auth::privacy::PrivacyLevel::Standard,
            data_core::config::PrivacyLevel::Strict => data_auth::privacy::PrivacyLevel::Strict,
        },
        epsilon: config.privacy_epsilon,
        ..Default::default()
    };

    // Start P2P network
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(256);
    let net_handle = data_p2p::network::start(&config, identity.seed(), event_tx).await?;
    let dht = DhtIndex::with_privacy(net_handle, privacy_config.clone());

    // Start file watcher
    let mut watch_rx = data_p2p::watchdir::watch(&config.data_dir)?;

    // Handle network events
    let store_bg = store.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                data_p2p::network::NetworkEvent::NewMetadata(data) => {
                    if let Ok(Some(metadata)) = data_p2p::gossip::verify_and_parse(&data) {
                        info!(cid = %metadata.cid.0, title = %metadata.title, "received new dataset via gossip");
                        let _ = store_bg.put(&metadata);
                    }
                }
                data_p2p::network::NetworkEvent::PeerConnected(peer) => {
                    info!(%peer, "peer connected");
                }
            }
        }
    });

    // Handle file watcher events → auto-publish with privacy
    let identity2 = NodeIdentity::from_seed(identity.seed());
    let store_watch = store.clone();
    let cmd_tx = dht.handle().cmd_tx.clone();
    let local_peer_id = dht.handle().local_peer_id;
    let price_default = config.price_default;
    let ephemeral_dids = config.ephemeral_dids;
    let publish_privacy = privacy_config.clone();
    tokio::spawn(async move {
        while let Some(event) = watch_rx.recv().await {
            let path = match event {
                data_p2p::watchdir::WatchEvent::FileCreated(p) => p,
                data_p2p::watchdir::WatchEvent::FileModified(p) => p,
            };
            info!(file = %path.display(), "detected new file, auto-publishing...");
            let publish_dht = DhtIndex::new(data_p2p::network::NetworkHandle {
                cmd_tx: cmd_tx.clone(),
                local_peer_id,
            });
            match data_p2p::publish::publish_file_with_privacy(
                &path,
                &identity2,
                &publish_dht,
                &store_watch,
                AccessMode::Open,
                price_default,
                &publish_privacy,
                ephemeral_dids,
            )
            .await
            {
                Ok(m) => info!(cid = %m.cid.0, "auto-published"),
                Err(e) => tracing::warn!(err = %e, "auto-publish failed"),
            }
        }
    });

    // External catalog periodic sync
    if config.catalog_sync_enabled {
        let store_sync = store.clone();
        let refresh_secs = config.catalog_sync_interval_secs;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(refresh_secs));
            loop {
                interval.tick().await;
                if let Err(e) = sync_external_catalogs(&store_sync).await {
                    tracing::warn!(error = %e, "catalog sync failed");
                }
            }
        });
    }

    // Start embedded Web UI + MCP HTTP server
    let state = Arc::new(McpServer::new(
        AppState::with_full_config(
            NodeIdentity::from_seed(identity.seed()),
            DhtIndex::new(data_p2p::network::NetworkHandle {
                cmd_tx: dht.handle().cmd_tx.clone(),
                local_peer_id: dht.handle().local_peer_id,
            }),
            store,
            feedback_store,
            &config.payment,
            &config.external_duckdb,
            &config.external_postgresql,
            &config.external_sql,
        )
        .await,
    ));
    let http_port = 3927;
    info!("Web UI → http://localhost:{http_port}");
    tokio::spawn(async move {
        if let Err(e) = data_mcp_server::server::run_http(state, http_port).await {
            tracing::warn!(err = %e, "HTTP server error");
        }
    });

    info!("full node running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    Ok(())
}

fn cmd_mcp_install(client: Option<String>) -> Result<()> {
    match client {
        Some(name) => {
            let c = mcp_install::Client::parse(&name).context(format!(
                "unknown client '{name}'. Use: claude, cursor, windsurf, kiro, codex, openclaw"
            ))?;
            mcp_install::install(c)
        }
        None => {
            mcp_install::list_detected();
            Ok(())
        }
    }
}

fn cmd_mcp_uninstall(client: &str) -> Result<()> {
    let c = mcp_install::Client::parse(client).context(format!(
        "unknown client '{client}'. Use: claude, cursor, windsurf, kiro, codex, openclaw"
    ))?;
    mcp_install::uninstall(c)
}

async fn cmd_mcp(mode: String) -> Result<()> {
    if mode == "codex" {
        let state = Arc::new(McpServer::new(build_codex_state().await?));
        return data_mcp_server::server::run_stdio(state).await;
    }

    let (config, identity) = load_config_and_identity()?;
    let node_mode = if mode == "full" {
        NodeMode::Full
    } else {
        NodeMode::Light
    };
    info!(?node_mode, did = %identity.did.0, "starting MCP server");

    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let feedback_store = FeedbackStore::open(&NodeConfig::config_dir().join("feedback_db"))?;

    let privacy_config = data_auth::privacy::PrivacyConfig {
        level: match config.privacy_level {
            data_core::config::PrivacyLevel::Off => data_auth::privacy::PrivacyLevel::Off,
            data_core::config::PrivacyLevel::Standard => data_auth::privacy::PrivacyLevel::Standard,
            data_core::config::PrivacyLevel::Strict => data_auth::privacy::PrivacyLevel::Strict,
        },
        epsilon: config.privacy_epsilon,
        ..Default::default()
    };

    // Start P2P network
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(256);
    let net_handle = data_p2p::network::start(&config, identity.seed(), event_tx).await?;
    let dht = DhtIndex::with_privacy(net_handle, privacy_config);

    // Handle gossip → local store in background
    let store_bg = store.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let data_p2p::network::NetworkEvent::NewMetadata(data) = event {
                if let Ok(Some(metadata)) = data_p2p::gossip::verify_and_parse(&data) {
                    let _ = store_bg.put(&metadata);
                }
            }
        }
    });

    let state = Arc::new(McpServer::new(
        AppState::with_full_config(
            NodeIdentity::from_seed(identity.seed()),
            dht,
            store,
            feedback_store,
            &config.payment,
            &config.external_duckdb,
            &config.external_postgresql,
            &config.external_sql,
        )
        .await,
    ));

    if mode == "http" {
        data_mcp_server::server::run_http(state, 3927).await
    } else {
        data_mcp_server::server::run_stdio(state).await
    }
}

async fn build_codex_state() -> Result<AppState> {
    match try_build_codex_state_from_local_store().await {
        Ok(state) => Ok(state),
        Err(e) => {
            tracing::warn!(
                err = %e,
                "local node state unavailable for codex MCP, falling back to session state"
            );
            AppState::for_codex().await
        }
    }
}

async fn try_build_codex_state_from_local_store() -> Result<AppState> {
    let (config, identity) = load_config_and_identity()?;
    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let feedback_store = FeedbackStore::open(&NodeConfig::config_dir().join("feedback_db"))?;
    Ok(AppState::for_local_store(
        NodeIdentity::from_seed(identity.seed()),
        store,
        feedback_store,
        &config.payment,
    )
    .await)
}

fn load_config_and_identity() -> Result<(NodeConfig, NodeIdentity)> {
    let config_path = NodeConfig::config_path();
    if !config_path.exists() {
        anyhow::bail!(
            "Node not initialized. Run `data-node init` first.\nExpected config at: {}",
            config_path.display()
        );
    }
    let config_str = std::fs::read_to_string(&config_path)?;
    let config: NodeConfig = toml::from_str(&config_str)?;

    let id_path = NodeConfig::identity_path();
    let seed_bytes = std::fs::read(&id_path)?;
    if seed_bytes.len() != 32 {
        anyhow::bail!("Invalid identity file at {}", id_path.display());
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&seed_bytes);
    let identity = NodeIdentity::from_seed(&seed);

    Ok((config, identity))
}

fn shellexpand(s: String) -> std::path::PathBuf {
    if let Some(stripped) = s.strip_prefix('~') {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped.strip_prefix('/').unwrap_or(stripped));
        }
    }
    std::path::PathBuf::from(s)
}

async fn sync_external_catalogs(store: &MetadataStore) -> Result<()> {
    use data_core::metadata::{DatasetMetadata, Provenance};
    use data_core::types::{AccessMode, DataSource, SearchResult};
    use data_search::adapters::{DefiLlamaAdapter, RwaXyzAdapter};

    fn search_result_to_metadata(r: &SearchResult, _source: DataSource) -> DatasetMetadata {
        DatasetMetadata {
            cid: r.cid.clone(),
            info_hash: None,
            title: r.title.clone(),
            description: r.description.clone(),
            tags: r.tags.clone(),
            data_type: r.data_type,
            schema: r.schema.clone(),
            stats: None,
            video_meta: None,
            access: AccessMode::Open,
            price: r.price.clone(),
            license: r.license.clone(),
            provider: r.provider.clone(),
            signature: String::new(),
            provenance: Provenance::Original,
            created_at: r.created_at,
            updated_at: chrono::Utc::now(),
            verifiable_credential: None,
            source_attributes: r.source_attributes.clone(),
        }
    }

    let defillama = DefiLlamaAdapter::default();
    for result in defillama
        .fetch_full_stablecoin_catalog()
        .await
        .unwrap_or_default()
    {
        let metadata = search_result_to_metadata(&result, DataSource::DefiLlama);
        let _ = store.put(&metadata);
    }

    let rwa = RwaXyzAdapter::default();
    for result in rwa.fetch_full_treasury_catalog().await.unwrap_or_default() {
        let metadata = search_result_to_metadata(&result, DataSource::RwaXyz);
        let _ = store.put(&metadata);
    }

    info!("catalog sync completed");
    Ok(())
}
