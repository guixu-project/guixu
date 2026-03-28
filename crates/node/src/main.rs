use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use data_core::config::{NodeConfig, NodeMode};
use data_core::identity::NodeIdentity;
use data_core::types::AccessMode;
use data_mcp_server::server::AppState;
use data_p2p::dht::DhtIndex;
use data_storage::feedback_store::FeedbackStore;
use data_storage::metadata_store::MetadataStore;

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
        #[arg(long, default_value = "light")]
        mode: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // --- Logging: stderr (human) + file (JSON, daily rotation) ---
    let log_dir = NodeConfig::config_dir().join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let file_appender = tracing_appender::rolling::daily(&log_dir, "guixu.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        // stderr: compact human-readable
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false)
                .compact(),
        )
        // file: structured JSON, all fields
        .with(
            fmt::layer()
                .with_writer(file_writer)
                .json()
                .with_span_list(false),
        )
        .with(env_filter)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { data_dir } => cmd_init(data_dir)?,
        Commands::Start => cmd_start().await?,
        Commands::Mcp { mode } => cmd_mcp(mode).await?,
    }

    Ok(())
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

    // Start embedded Web UI + MCP HTTP server
    let disabled_adapters = config.disabled_adapters.clone();
    let state = Arc::new(
        AppState::with_payment_config(
            NodeIdentity::from_seed(identity.seed()),
            DhtIndex::new(data_p2p::network::NetworkHandle {
                cmd_tx: dht.handle().cmd_tx.clone(),
                local_peer_id: dht.handle().local_peer_id,
            }),
            store,
            feedback_store,
            &config.payment,
            &disabled_adapters,
        )
        .await,
    );
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

async fn cmd_mcp(mode: String) -> Result<()> {
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

    let state = Arc::new(
        AppState::with_payment_config(
            NodeIdentity::from_seed(identity.seed()),
            dht,
            store,
            feedback_store,
            &config.payment,
            &config.disabled_adapters,
        )
        .await,
    );

    if mode == "http" {
        data_mcp_server::server::run_http(state, 3927).await
    } else {
        data_mcp_server::server::run_stdio(state).await
    }
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
