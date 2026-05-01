// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use data_core::config::{NodeConfig, NodeMode};
use data_core::env::load_local_settings;
use data_core::identity::NodeIdentity;
use data_core::types::{AccessMode, DatasetCid};
use data_mcp_server::host_sampling::HostSamplingRuntime;
use data_mcp_server::server::{AppState, McpServer};
use data_p2p::dht::DhtIndex;
use data_storage::feedback_store::FeedbackStore;
use data_storage::job_store::JobStore;
use data_storage::metadata_store::MetadataStore;

#[cfg(unix)]
const SIGTERM: libc::c_int = libc::SIGTERM;
#[cfg(unix)]
const SIGKILL: libc::c_int = libc::SIGKILL;
#[cfg(unix)]
fn kill(pid: u32, sig: libc::c_int) -> std::io::Result<()> {
    // SAFETY: libc::kill sends a signal to a process. We validate pid != 0 at call sites.
    unsafe { libc::kill(pid as libc::pid_t, sig) };
    Ok(())
}

#[cfg(not(unix))]
fn kill(pid: u32, _sig: i32) -> std::io::Result<()> {
    std::process::Command::new("kill")
        .arg("-TERM".to_string())
        .arg(pid.to_string())
        .output()?;
    Ok(())
}

mod chat;
mod mcp_install;
mod service_files;
mod watchdog;

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
    Start {
        /// Run as background daemon.
        #[arg(short, long)]
        daemon: bool,
    },
    /// Stop a running daemon.
    Stop,
    /// Show node status.
    Status,
    /// Publish a local file to the P2P network.
    Publish {
        /// Path to the file to publish.
        path: String,
        /// Price in USDC (0 = free).
        #[arg(long, default_value = "0")]
        price: f64,
        /// SPDX license identifier.
        #[arg(long, default_value = "CC-BY-4.0")]
        license: String,
        /// Comma-separated tags.
        #[arg(long)]
        tags: Option<String>,
    },
    /// Remove a dataset from the P2P network.
    Unpublish {
        /// CID of the dataset to unpublish.
        cid: String,
    },
    /// List published datasets.
    List {
        /// Show only datasets published by this node.
        #[arg(long)]
        mine: bool,
        /// Output format.
        #[arg(long, default_value = "table")]
        format: String,
    },
    /// Preview a dataset (first N rows).
    Preview {
        /// CID of the dataset.
        cid: String,
        /// Number of rows to preview.
        #[arg(long, default_value = "20")]
        rows: usize,
    },
    /// Run as MCP server only (for Agent integration).
    Mcp {
        #[command(subcommand)]
        action: Option<McpAction>,

        #[arg(long, default_value = "light", global = true)]
        mode: String,
    },
    /// Start an interactive agent chat session.
    Chat {
        /// LLM provider (openai, anthropic, ollama).
        #[arg(long, default_value = "openai")]
        provider: String,

        /// Model name (e.g. gpt-4, claude-3-sonnet, llama3).
        #[arg(long)]
        model: Option<String>,

        /// API key (overrides env var).
        #[arg(long)]
        api_key: Option<String>,

        /// API base URL (for custom endpoints).
        #[arg(long)]
        api_base: Option<String>,
    },
    /// Manage AI agent traces (import, export, query, sanitize).
    Trace {
        #[command(subcommand)]
        action: TraceAction,
    },
    /// Control external AI agents (hermes, openclaw, etc.).
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
}

#[derive(Subcommand)]
enum McpAction {
    /// Register Guixu MCP with an AI client.
    Install {
        /// Target agent name (e.g. codex, cursor, claude-code, opencode, openclaw).
        /// Custom agents can be defined in ~/.data-node/agents.toml.
        client: Option<String>,
    },
    /// Remove Guixu MCP from an AI client.
    Uninstall {
        /// Target agent name
        client: String,
    },
}

/// Trace management subcommands.
#[derive(Subcommand)]
enum TraceAction {
    /// Import traces from an external provider (OpenAI or Claude).
    Import {
        /// Provider: openai or claude
        #[arg(long)]
        provider: String,

        /// Path to the trace file (JSONL format)
        #[arg(long)]
        file: String,

        /// Path to the DuckDB trace database
        #[arg(long, default_value = "traces.duckdb")]
        db: String,

        /// Skip malformed records (default: true)
        #[arg(long, default_value = "true")]
        skip_errors: bool,
    },
    /// Export traces with sanitization.
    Export {
        /// Source of traces to export: guixu, openai, or claude
        #[arg(long, default_value = "guixu")]
        source: String,

        /// Output file path
        #[arg(long)]
        output: String,

        /// Sanitization level: off, standard, strict
        #[arg(long, default_value = "standard")]
        level: String,

        /// Path to the DuckDB trace database
        #[arg(long, default_value = "traces.duckdb")]
        db: String,
    },
    /// List recent traces.
    List {
        /// Source filter: guixu, openai, or claude (default: all)
        #[arg(long)]
        source: Option<String>,

        /// Maximum number of traces to show
        #[arg(long, default_value = "20")]
        limit: i64,

        /// Path to the DuckDB trace database
        #[arg(long, default_value = "traces.duckdb")]
        db: String,
    },
    /// Query spans from a specific trace.
    Query {
        /// Trace ID
        #[arg(long)]
        trace_id: String,

        /// Source of traces
        #[arg(long, default_value = "guixu")]
        source: String,

        /// Path to the DuckDB trace database
        #[arg(long, default_value = "traces.duckdb")]
        db: String,
    },
}

/// External agent control subcommands.
#[derive(Subcommand)]
enum AgentAction {
    /// List configured external agents.
    List,
    /// Add a new external agent configuration.
    Add {
        /// Agent identifier.
        #[arg(long)]
        id: String,
        /// Agent name.
        #[arg(long)]
        name: String,
        /// Agent type: http or cli.
        #[arg(long)]
        agent_type: String,
        /// Connection URL (for http type).
        #[arg(long)]
        url: Option<String>,
        /// Executable path (for cli type).
        #[arg(long)]
        executable: Option<String>,
    },
    /// Remove an external agent configuration.
    Remove {
        /// Agent identifier.
        id: String,
    },
    /// Execute a task on an external agent.
    Execute {
        /// Agent identifier.
        #[arg(long)]
        agent: String,
        /// Task description or prompt.
        #[arg(long)]
        prompt: String,
        /// Timeout in seconds.
        #[arg(long, default_value = "60")]
        timeout: u64,
        /// Additional parameters (JSON format).
        #[arg(long)]
        params: Option<String>,
    },
    /// Check health of an external agent.
    Health {
        /// Agent identifier.
        agent: String,
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
        Commands::Start { daemon } => {
            if daemon {
                cmd_start_daemon()?;
            } else {
                cmd_start().await?;
            }
        }
        Commands::Stop => cmd_stop()?,
        Commands::Status => cmd_status().await?,
        Commands::Publish {
            path,
            price,
            license: _license,
            tags,
        } => cmd_publish(&path, price, tags).await?,
        Commands::Unpublish { cid } => cmd_unpublish(&cid).await?,
        Commands::List { mine, format } => cmd_list(mine, &format)?,
        Commands::Preview { cid, rows } => cmd_preview(&cid, rows).await?,
        Commands::Mcp { action, mode } => match action {
            Some(McpAction::Install { client }) => cmd_mcp_install(client)?,
            Some(McpAction::Uninstall { client }) => cmd_mcp_uninstall(&client)?,
            None => cmd_mcp(mode).await?,
        },
        Commands::Chat {
            provider,
            model,
            api_key,
            api_base,
        } => cmd_chat(provider, model, api_key, api_base).await?,
        Commands::Trace { action } => cmd_trace(action)?,
        Commands::Agent { action } => cmd_agent(action).await?,
    }

    Ok(())
}

fn cmd_trace(action: TraceAction) -> Result<()> {
    use data_storage::trace_import::{ClaudeImporter, OpenAiImporter, TraceImporter};
    use data_storage::trace_sanitizer::{SanitizationLevel, TraceSanitizer};
    use data_storage::trace_store::TraceStore;
    use std::path::Path;

    match action {
        TraceAction::Import {
            provider,
            file,
            db,
            skip_errors,
        } => {
            let store = TraceStore::open(Path::new(&db))?;
            let config = data_storage::trace_import::ImporterConfig {
                skip_errors,
                ..Default::default()
            };

            let report = match provider.as_str() {
                "openai" => {
                    let importer = OpenAiImporter::new(config);
                    importer.import_file(Path::new(&file), &store)?
                }
                "claude" => {
                    let importer = ClaudeImporter::new(config);
                    importer.import_file(Path::new(&file), &store)?
                }
                _ => {
                    anyhow::bail!("Unknown provider: {}. Use 'openai' or 'claude'.", provider);
                }
            };

            println!(
                "Import complete: {} spans imported, {} traces, {} errors",
                report.spans_imported,
                report.traces_processed,
                report.errors.len()
            );
            if !report.errors.is_empty() {
                for e in &report.errors {
                    eprintln!("  line {}: {}", e.line, e.message);
                }
            }
        }
        TraceAction::Export {
            source,
            output,
            level,
            db,
        } => {
            let store = TraceStore::open(Path::new(&db))?;
            let sanitization_level = match level.as_str() {
                "off" => SanitizationLevel::Off,
                "standard" => SanitizationLevel::Standard,
                "strict" => SanitizationLevel::Strict,
                _ => anyhow::bail!(
                    "Unknown level: {}. Use 'off', 'standard', or 'strict'.",
                    level
                ),
            };

            let sanitizer = TraceSanitizer::new(sanitization_level);
            let report = sanitizer.export_traces(&store, &source, Path::new(&output))?;

            println!(
                "Export complete: {} spans exported, {} redactions (level={})",
                report.spans_exported, report.total_redactions, level
            );
        }
        TraceAction::List { source, limit, db } => {
            let store = TraceStore::open(Path::new(&db))?;
            let sources: Vec<&str> = if let Some(s) = &source {
                vec![s.as_str()]
            } else {
                vec!["guixu", "openai", "claude"]
            };

            for s in sources {
                let traces = store.list_traces(s, limit)?;
                if traces.is_empty() {
                    continue;
                }
                println!("\n=== {} traces (source={}) ===", traces.len(), s);
                for t in traces {
                    println!(
                        "  {}  spans={}  duration={:.2}ms  tokens={}/{}  {}",
                        t.trace_id,
                        t.span_count,
                        t.total_duration_ms,
                        t.total_input_tokens,
                        t.total_output_tokens,
                        t.last_span_time.format("%Y-%m-%d %H:%M")
                    );
                }
            }
        }
        TraceAction::Query {
            trace_id,
            source,
            db,
        } => {
            let store = TraceStore::open(Path::new(&db))?;
            let spans = store.get_trace_spans(&trace_id, &source)?;
            if spans.is_empty() {
                println!("No spans found for trace_id={} source={}", trace_id, source);
                return Ok(());
            }
            println!("Trace {} ({} spans):\n", trace_id, spans.len());
            for s in spans {
                println!(
                    "  {:20} {:8} {:>12}ms  in={:>5} out={:>5}  {}",
                    s.span_id.chars().take(20).collect::<String>(),
                    s.span_type.as_str(),
                    format!("{:.2}", s.duration_ms),
                    s.input_tokens.unwrap_or(0),
                    s.output_tokens.unwrap_or(0),
                    s.model.as_deref().unwrap_or("-"),
                );
            }
        }
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

    // Crash recovery: clean stale PID file
    let pid_path = NodeConfig::pid_path();
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if kill(pid, 0).is_err() {
                    info!("cleaning stale PID file");
                    std::fs::remove_file(&pid_path).ok();
                }
            }
        }
    }

    // Write PID file
    std::fs::write(&pid_path, std::process::id().to_string())?;

    // Generate platform service files (first run)
    if let Err(e) = service_files::generate_service_files() {
        tracing::warn!(error = %e, "service file generation failed (non-fatal)");
    }

    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let feedback_store = FeedbackStore::open(&NodeConfig::config_dir().join("feedback_db"))?;
    let job_store = JobStore::open(&NodeConfig::config_dir().join("job_db"))?;

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

    // Seed recovery: restore all persisted seeds
    let seeds = store.list_seeds()?;
    if !seeds.is_empty() {
        info!(count = seeds.len(), "restoring seeds from previous session");
        let download_dir = NodeConfig::config_dir().join("downloads");
        if let Ok(engine) = data_p2p::torrent::TorrentEngine::new(download_dir).await {
            let restored = engine.restore_seeds(&seeds).await;
            info!(restored = restored.len(), "seeds restored");
        }
    }

    // Start file watcher
    let mut watch_rx = data_p2p::watchdir::watch(&config.data_dir)?;

    // Handle network events
    let store_bg = store.clone();
    let identity_seed_bg = *identity.seed();
    let cmd_tx_bg = dht.handle().cmd_tx.clone();
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
                data_p2p::network::NetworkEvent::IncomingSampleRequest {
                    peer,
                    channel_id,
                    request,
                } => {
                    info!(%peer, cid = %request.cid.0, "incoming sample request");
                    let store_ref = store_bg.clone();
                    let seed = identity_seed_bg;
                    let cmd_ref = cmd_tx_bg.clone();
                    tokio::task::spawn_blocking(move || {
                        let id = NodeIdentity::from_seed(&seed);
                        match data_p2p::sample_server::handle_sample_request(
                            &request, &store_ref, &id,
                        ) {
                            Ok(Some(resp)) => {
                                let _ = cmd_ref.blocking_send(
                                    data_p2p::network::NetworkCommand::SampleResponse {
                                        channel_id,
                                        response: resp,
                                    },
                                );
                            }
                            Ok(None) => {
                                tracing::debug!(channel_id, "sample request: no data");
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "sample request handler failed");
                            }
                        }
                    });
                }
                data_p2p::network::NetworkEvent::IncomingAccessRequest {
                    peer,
                    channel_id,
                    request,
                } => {
                    info!(%peer, cid = %request.cid.0, "incoming access request");
                    let store_ref = store_bg.clone();
                    let cmd_ref = cmd_tx_bg.clone();
                    tokio::task::spawn_blocking(move || {
                        match data_p2p::access_control::handle_access_request(
                            &request, &store_ref, false,
                        ) {
                            Ok(Some(grant)) => {
                                let _ = cmd_ref.blocking_send(
                                    data_p2p::network::NetworkCommand::AccessResponse {
                                        channel_id,
                                        response: grant,
                                    },
                                );
                            }
                            Ok(None) => {
                                tracing::debug!(channel_id, "access request: denied");
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "access request handler failed");
                            }
                        }
                    });
                }
                _ => {}
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

    // Start watchdog
    let watchdog = watchdog::WatchdogTask::new(
        dht.handle().cmd_tx.clone(),
        3927,
        config.data_dir.clone(),
        config.daemon.clone(),
    );
    tokio::spawn(watchdog.run());

    // Start embedded Web UI + MCP HTTP server
    let mut mcp_server = McpServer::new(
        AppState::with_full_config(
            NodeIdentity::from_seed(identity.seed()),
            DhtIndex::new(data_p2p::network::NetworkHandle {
                cmd_tx: dht.handle().cmd_tx.clone(),
                local_peer_id: dht.handle().local_peer_id,
            }),
            store,
            feedback_store,
            job_store,
            &config.payment,
            &config.external_duckdb,
            &config.external_postgresql,
            &config.external_sql,
        )
        .await,
    );
    if let Err(e) = mcp_server.init_trace_pool(4) {
        tracing::warn!(err = %e, "failed to init trace pool, falling back to per-request connections");
    }
    let state = Arc::new(mcp_server);
    let http_port = 3927;
    info!("Web UI → http://localhost:{http_port}");
    tokio::spawn(async move {
        if let Err(e) = data_mcp_server::server::run_http(state, http_port).await {
            tracing::warn!(err = %e, "HTTP server error");
        }
    });

    info!("full node running. Press Ctrl+C to stop.");

    // SIGTERM / Ctrl+C graceful shutdown
    tokio::signal::ctrl_c().await?;
    info!("shutting down...");

    // 1. Wait briefly for in-flight transfers
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // 2. Clean up PID file
    std::fs::remove_file(&pid_path).ok();

    info!("shutdown complete");
    Ok(())
}

fn cmd_mcp_install(client: Option<String>) -> Result<()> {
    match client {
        Some(name) => mcp_install::install(&name),
        None => {
            mcp_install::list_detected();
            Ok(())
        }
    }
}

fn cmd_mcp_uninstall(client: &str) -> Result<()> {
    mcp_install::uninstall(client)
}

async fn cmd_chat(
    provider: String,
    model: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> Result<()> {
    chat::run(provider, model, api_key, api_base).await
}

async fn cmd_mcp(mode: String) -> Result<()> {
    let search_workers = resolve_search_workers(&mode);
    if mode == "codex" {
        let sampling_runtime = Arc::new(HostSamplingRuntime::new());
        let state = Arc::new(McpServer::new(
            build_codex_state(search_workers, sampling_runtime).await?,
        ));
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
    let job_store = JobStore::open(&NodeConfig::config_dir().join("job_db"))?;

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

    let mut mcp_server = McpServer::new(
        AppState::with_full_config_with_search_workers(
            NodeIdentity::from_seed(identity.seed()),
            dht,
            store,
            feedback_store,
            job_store,
            &config.payment,
            &config.external_duckdb,
            &config.external_postgresql,
            &config.external_sql,
            search_workers,
        )
        .await,
    );
    if let Err(e) = mcp_server.init_trace_pool(4) {
        tracing::warn!(err = %e, "failed to init trace pool, falling back to per-request connections");
    }
    let state = Arc::new(mcp_server);

    if mode == "http" {
        data_mcp_server::server::run_http(state, 3927).await
    } else {
        data_mcp_server::server::run_stdio(state).await
    }
}

fn resolve_search_workers(mode: &str) -> usize {
    const SEARCH_WORKERS_ENV: &str = "GUIXU_SEARCH_WORKERS";

    match std::env::var(SEARCH_WORKERS_ENV) {
        Ok(raw) => match raw.trim().parse::<usize>() {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(
                    env_var = SEARCH_WORKERS_ENV,
                    value = %raw,
                    error = %error,
                    "invalid search worker configuration; using mode default"
                );
                default_search_workers(mode)
            }
        },
        Err(std::env::VarError::NotPresent) => default_search_workers(mode),
        Err(error) => {
            tracing::warn!(
                env_var = SEARCH_WORKERS_ENV,
                error = %error,
                "failed to read search worker configuration; using mode default"
            );
            default_search_workers(mode)
        }
    }
}

fn default_search_workers(mode: &str) -> usize {
    if mode == "codex" {
        4
    } else {
        0
    }
}

async fn build_codex_state(
    search_workers: usize,
    sampling_runtime: Arc<HostSamplingRuntime>,
) -> Result<AppState> {
    match try_build_codex_state_from_local_store(search_workers, sampling_runtime.clone()).await {
        Ok(state) => Ok(state),
        Err(e) => {
            tracing::warn!(
                err = %e,
                "local node state unavailable for codex MCP, falling back to session state"
            );
            AppState::for_codex_with_search_workers(search_workers)
                .await
                .map(|state| state.with_sampling_runtime(sampling_runtime))
        }
    }
}

async fn try_build_codex_state_from_local_store(
    search_workers: usize,
    sampling_runtime: Arc<HostSamplingRuntime>,
) -> Result<AppState> {
    let (config, identity) = load_config_and_identity()?;
    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let feedback_store = FeedbackStore::open(&NodeConfig::config_dir().join("feedback_db"))?;
    Ok(AppState::for_local_store_with_search_workers(
        NodeIdentity::from_seed(identity.seed()),
        store,
        feedback_store,
        &config.payment,
        search_workers,
    )
    .await
    .with_sampling_runtime(sampling_runtime))
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

fn cmd_start_daemon() -> Result<()> {
    use std::fs::{self, File};

    let pid_path = NodeConfig::pid_path();
    if pid_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if pid != 0 && kill(pid, 0).is_ok() {
                    anyhow::bail!("Daemon already running with PID {pid}. Use `guixu stop` first.");
                }
            }
        }
        fs::remove_file(&pid_path).ok();
    }

    let log_dir = NodeConfig::config_dir().join("logs");
    fs::create_dir_all(&log_dir)?;

    match daemonize::Daemonize::new()
        .pid_file(&pid_path)
        .stdout(File::create(log_dir.join("stdout.log"))?)
        .stderr(File::create(log_dir.join("stderr.log"))?)
        .start()
    {
        Ok(()) => {
            // We are now in the child process
            println!("Daemon started with PID {}", std::process::id());
            Ok(())
        }
        Err(e) => anyhow::bail!("Failed to start daemon: {e}"),
    }
}

fn cmd_stop() -> Result<()> {
    use std::fs;

    let pid_path = NodeConfig::pid_path();
    if !pid_path.exists() {
        anyhow::bail!("PID file not found. Is the daemon running?");
    }

    let pid_str = fs::read_to_string(&pid_path)?;
    let pid = pid_str.trim().parse::<u32>()?;

    if pid == 0 {
        anyhow::bail!("Invalid PID");
    }

    if kill(pid, 0).is_err() {
        fs::remove_file(&pid_path)?;
        println!("Daemon was not running (stale PID file removed)");
        return Ok(());
    }

    if kill(pid, SIGTERM).is_err() {
        anyhow::bail!("Failed to send SIGTERM to {pid}");
    }

    let timeout = std::time::Duration::from_secs(5);
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if kill(pid, 0).is_err() {
            fs::remove_file(&pid_path)?;
            println!("Daemon stopped gracefully");
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("Daemon did not stop gracefully, forcing...");
    kill(pid, SIGKILL).ok();
    fs::remove_file(&pid_path)?;
    println!("Daemon killed");
    Ok(())
}

async fn cmd_status() -> Result<()> {
    let pid_path = NodeConfig::pid_path();
    if !pid_path.exists() {
        println!("Daemon not running (no PID file)");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid = pid_str.trim().parse::<u32>()?;

    if kill(pid, 0).is_err() {
        println!("Daemon not running (stale PID file)");
        return Ok(());
    }

    println!("Daemon running: PID {pid}");

    let (config, identity) = load_config_and_identity()?;
    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let seeds = store.list_seeds()?;
    println!("Published datasets: {}", seeds.len());
    for seed in &seeds {
        println!(
            "  {} - {} ({} bytes)",
            seed.cid.0, seed.title, seed.size_bytes
        );
    }

    let client = reqwest::Client::new();
    if let Ok(resp) = client
        .get("http://127.0.0.1:3927/api/node/status")
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(body) = resp.text().await {
                println!("HTTP status: {}", body);
            }
        }
    }

    println!("Node DID: {}", identity.did.0);
    println!("Data dir: {}", config.data_dir.display());
    Ok(())
}

async fn cmd_publish(path: &str, price: f64, _tags: Option<String>) -> Result<()> {
    let (config, identity) = load_config_and_identity()?;
    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let privacy_config = data_auth::privacy::PrivacyConfig::default();

    let path = shellexpand(path.into());
    if !path.exists() {
        anyhow::bail!("File not found: {}", path.display());
    }

    // Start temporary P2P session
    let (event_tx, _event_rx) = tokio::sync::mpsc::channel(256);
    let net_handle = data_p2p::network::start(&config, identity.seed(), event_tx).await?;
    let dht = DhtIndex::with_privacy(net_handle, privacy_config);

    let access = if price > 0.0 {
        AccessMode::Paid
    } else {
        AccessMode::Open
    };

    let metadata = data_p2p::publish::publish_file_with_privacy(
        &path,
        &identity,
        &dht,
        &store,
        access,
        price,
        &data_auth::privacy::PrivacyConfig::default(),
        config.ephemeral_dids,
    )
    .await?;

    // Create torrent and start seeding
    let download_dir = NodeConfig::config_dir().join("downloads");
    if let Ok(engine) = data_p2p::torrent::TorrentEngine::new(download_dir).await {
        match engine.create_torrent(&path).await {
            Ok(info_hash) => {
                println!("✅ Published: cid={}", metadata.cid.0);
                println!("   Seeding via BT: info_hash={info_hash}");
            }
            Err(e) => {
                println!("✅ Published: cid={}", metadata.cid.0);
                println!("   ⚠ Torrent creation failed: {e}");
            }
        }
    } else {
        println!("✅ Published: cid={}", metadata.cid.0);
        if let Some(ref hash) = metadata.info_hash {
            println!("   info_hash={hash}");
        }
    }
    Ok(())
}

async fn cmd_unpublish(cid: &str) -> Result<()> {
    let (config, identity) = load_config_and_identity()?;
    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let dataset_cid = DatasetCid(cid.to_string());

    let metadata = store
        .get(&dataset_cid)?
        .ok_or_else(|| anyhow::anyhow!("Dataset not found: {}", cid))?;

    // 1. Delete SeedRecord and stop BT seeding
    if let Some(ref info_hash) = metadata.info_hash {
        store.delete_seed(info_hash)?;
    }

    // 2. Remove from DHT
    let (event_tx, _event_rx) = tokio::sync::mpsc::channel(256);
    let net_handle = data_p2p::network::start(&config, identity.seed(), event_tx).await?;
    let dht = DhtIndex::new(net_handle);
    // Overwrite DHT record with empty value to effectively delete
    let key = format!("meta:{}", cid).into_bytes();
    dht.handle().dht_put(key, vec![]).await?;

    // 3. Mark as unpublished in local store
    store.mark_unpublished(&dataset_cid)?;

    println!("✅ Unpublished: cid={cid}");
    Ok(())
}

fn cmd_list(mine: bool, format: &str) -> Result<()> {
    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let (_config, identity) = load_config_and_identity()?;

    let datasets = store.list_all()?;
    let my_datasets: Vec<_> = if mine {
        datasets
            .into_iter()
            .filter(|m| m.provider.0 == identity.did.0)
            .collect()
    } else {
        datasets
    };

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&my_datasets)?);
    } else {
        println!(
            "{:12} {:30} {:>10} {:>12} {:>8} {:>8}",
            "CID", "TITLE", "ROWS", "SIZE", "PRICE", "STATUS"
        );
        println!("{}", "-".repeat(90));
        for m in &my_datasets {
            let size_str = format_size(m.schema.size_bytes);
            let price_str = if m.price.is_free() {
                "free".to_string()
            } else {
                format!("${:.2}", m.price.amount)
            };
            let status = if m.info_hash.is_some() {
                "seeding"
            } else {
                "local"
            };
            println!(
                "{:12} {:30} {:>10} {:>12} {:>8} {:>8}",
                &m.cid.0[..12.min(m.cid.0.len())],
                &m.title[..30.min(m.title.len())],
                m.schema.row_count,
                size_str,
                price_str,
                status
            );
        }
    }
    Ok(())
}

async fn cmd_preview(cid: &str, rows: usize) -> Result<()> {
    let store = MetadataStore::open(&NodeConfig::db_path())?;
    let dataset_cid = DatasetCid(cid.to_string());

    let metadata = store
        .get(&dataset_cid)?
        .ok_or_else(|| anyhow::anyhow!("Dataset not found: {}", cid))?;

    let file_path = store
        .get_file_path(&dataset_cid)?
        .ok_or_else(|| anyhow::anyhow!("Local file not found for {}", cid))?;

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "csv" | "tsv" => {
            let separator = if ext == "tsv" { '\t' } else { ',' };
            let content = std::fs::read(&file_path)?;
            let text = String::from_utf8_lossy(&content);
            let lines: Vec<&str> = text.lines().take(rows + 1).collect();

            if lines.is_empty() {
                println!("Empty file");
                return Ok(());
            }

            // Compute column widths for aligned table output
            let all_cols: Vec<Vec<&str>> = lines
                .iter()
                .map(|line| line.split(separator).collect())
                .collect();
            let num_cols = all_cols.first().map(|r| r.len()).unwrap_or(0);
            let mut widths = vec![0usize; num_cols];
            for row in &all_cols {
                for (i, col) in row.iter().enumerate() {
                    if i < widths.len() {
                        widths[i] = widths[i].max(col.len()).min(30);
                    }
                }
            }

            // Print header
            if let Some(header) = all_cols.first() {
                let h: Vec<String> = header
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("{:w$}", c, w = widths.get(i).copied().unwrap_or(10)))
                    .collect();
                println!("{}", h.join("  "));
                let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
                println!("{}", sep.join("  "));
            }

            // Print data rows
            for row in all_cols.iter().skip(1) {
                let r: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let w = widths.get(i).copied().unwrap_or(10);
                        let truncated = if c.len() > w { &c[..w] } else { c };
                        format!("{:w$}", truncated, w = w)
                    })
                    .collect();
                println!("{}", r.join("  "));
            }
        }
        "json" => {
            let content = std::fs::read(&file_path)?;
            let text = String::from_utf8_lossy(&content);
            let preview: String = text.chars().take(rows * 100).collect();
            println!("{preview}");
        }
        _ => {
            anyhow::bail!("Preview not supported for {} files", ext);
        }
    }

    println!("\n--- Schema ---");
    for col in &metadata.schema.columns {
        println!("  {} ({})", col.name, col.dtype);
    }
    println!("Columns: {}", metadata.schema.columns.len());
    println!("Row count: {}", metadata.schema.row_count);
    println!("Size: {}", format_size(metadata.schema.size_bytes));
    Ok(())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
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
            version: None,
            previous_version: None,
            verifiable_credential: None,
            source_attributes: r.source_attributes.clone(),
        }
    }

    let defillama = DefiLlamaAdapter::default();
    let rwa = RwaXyzAdapter::default();

    let (defillama_results, rwa_results) = tokio::join!(
        defillama.fetch_full_stablecoin_catalog(),
        rwa.fetch_full_treasury_catalog()
    );

    for result in defillama_results.unwrap_or_default() {
        let metadata = search_result_to_metadata(&result, DataSource::DefiLlama);
        let _ = store.put(&metadata);
    }

    for result in rwa_results.unwrap_or_default() {
        let metadata = search_result_to_metadata(&result, DataSource::RwaXyz);
        let _ = store.put(&metadata);
    }

    info!("catalog sync completed");
    Ok(())
}

async fn cmd_agent(action: AgentAction) -> Result<()> {
    use data_external_agents::config::{
        CliConnection, ConnectionConfig, ExternalAgentConfig, HttpConnection,
    };
    use data_external_agents::traits::AgentFactory;
    use data_external_agents::types::AgentTask;
    use std::collections::HashMap;
    use std::path::PathBuf;

    // Load agent configurations from file
    let config_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".data-node")
        .join("external-agents.json");

    let mut agents: HashMap<String, ExternalAgentConfig> = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };

    match action {
        AgentAction::List => {
            if agents.is_empty() {
                println!("No external agents configured.");
            } else {
                println!("Configured external agents:");
                for (id, config) in &agents {
                    println!("  {} - {} ({})", id, config.name, config.agent_type);
                }
            }
        }
        AgentAction::Add {
            id,
            name,
            agent_type,
            url,
            executable,
        } => {
            let connection = match agent_type.as_str() {
                "http" => {
                    let base_url =
                        url.ok_or_else(|| anyhow::anyhow!("URL is required for HTTP agent type"))?;
                    ConnectionConfig::Http(HttpConnection {
                        base_url,
                        timeout_secs: 30,
                        verify_ssl: true,
                        headers: HashMap::new(),
                    })
                }
                "cli" => {
                    let exec_path = executable.ok_or_else(|| {
                        anyhow::anyhow!("Executable path is required for CLI agent type")
                    })?;
                    ConnectionConfig::Cli(CliConnection {
                        executable: PathBuf::from(exec_path),
                        working_dir: None,
                        env_vars: HashMap::new(),
                        shell: None,
                        capture_stderr: true,
                    })
                }
                _ => anyhow::bail!("Unknown agent type: {}", agent_type),
            };

            let config = ExternalAgentConfig {
                id: id.clone(),
                name,
                agent_type,
                connection,
                default_timeout_secs: 60,
                max_retries: 3,
                auth: None,
                extra: HashMap::new(),
            };

            agents.insert(id.clone(), config);

            // Save to file
            let content = serde_json::to_string_pretty(&agents)?;
            std::fs::create_dir_all(config_path.parent().unwrap())?;
            std::fs::write(&config_path, content)?;

            println!("Agent '{}' added successfully.", id);
        }
        AgentAction::Remove { id } => {
            if agents.remove(&id).is_some() {
                let content = serde_json::to_string_pretty(&agents)?;
                std::fs::write(&config_path, content)?;
                println!("Agent '{}' removed.", id);
            } else {
                println!("Agent '{}' not found.", id);
            }
        }
        AgentAction::Execute {
            agent,
            prompt,
            timeout,
            params,
        } => {
            let config = agents
                .get(&agent)
                .ok_or_else(|| anyhow::anyhow!("Agent '{}' not found", agent))?;

            let agent_instance = AgentFactory::create(config)?;

            let mut task = AgentTask::new(&prompt).with_timeout(timeout);

            // Add parameters if provided
            if let Some(params_str) = params {
                let params_value: serde_json::Value = serde_json::from_str(&params_str)?;
                if let Some(params_obj) = params_value.as_object() {
                    for (key, value) in params_obj {
                        task = task.with_parameter(key, value.clone());
                    }
                }
            }

            println!("Executing task on agent '{}'...", agent);
            let response = agent_instance.execute_task(task).await?;

            if response.is_success() {
                println!("Task completed successfully:");
                if let Some(content) = &response.content {
                    println!("{}", content);
                }
            } else {
                println!("Task failed:");
                if let Some(error) = &response.error {
                    println!("Error: {}", error);
                }
            }
        }
        AgentAction::Health { agent } => {
            let config = agents
                .get(&agent)
                .ok_or_else(|| anyhow::anyhow!("Agent '{}' not found", agent))?;

            let agent_instance = AgentFactory::create(config)?;
            let health = agent_instance.health_check().await?;

            println!("Agent '{}' health status:", agent);
            println!("  Reachable: {}", health.is_reachable);
            if let Some(version) = &health.version {
                println!("  Version: {}", version);
            }
            if !health.metadata.is_empty() {
                println!("  Metadata:");
                for (key, value) in &health.metadata {
                    println!("    {}: {}", key, value);
                }
            }
        }
    }

    Ok(())
}
