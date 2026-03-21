//! Unimatrix knowledge engine entry point.

use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use sha2::{Digest, Sha256};
use unimatrix_adapt::{AdaptConfig, AdaptationService};
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{CoreError, EmbedConfig, Store, VectorAdapter, VectorConfig, VectorIndex};
use unimatrix_embed::NliModel;
use unimatrix_engine::confidence::ConfidenceParams;
use unimatrix_observe::domain::{DomainPack, DomainPackRegistry};
use unimatrix_server::error::ServerError;
use unimatrix_server::infra::audit::AuditLog;
use unimatrix_server::infra::categories::CategoryAllowlist;
use unimatrix_server::infra::config::{
    DomainPackConfig, UnimatrixConfig, load_config, resolve_confidence_params,
};
use unimatrix_server::infra::embed_handle::EmbedServiceHandle;
use unimatrix_server::infra::nli_handle::{NliConfig, NliServiceHandle};
use unimatrix_server::infra::pidfile;
use unimatrix_server::infra::rayon_pool::RayonPool;
use unimatrix_server::infra::registry::{AgentRegistry, Capability};
use unimatrix_server::infra::shutdown::{self, LifecycleHandles};
use unimatrix_server::infra::usage_dedup::UsageDedup;
use unimatrix_server::project;
use unimatrix_server::server::{PendingEntriesAnalysis, UnimatrixServer};
use unimatrix_server::uds_listener;

/// Timeout for waiting on a stale process to exit after SIGTERM.
/// Increased from 5s to 10s to accommodate heavier shutdown since vnc-006 (#92).
const STALE_PROCESS_TIMEOUT: Duration = Duration::from_secs(10);

/// Convert a TOML `DomainPackConfig` to a `DomainPack`.
///
/// The `rule_file` field is not yet implemented (W1-5 scope: built-in claude-code rules only).
/// External rule files will be supported in a follow-on feature.
fn domain_pack_from_config(cfg: &DomainPackConfig) -> DomainPack {
    DomainPack {
        source_domain: cfg.source_domain.clone(),
        event_types: cfg.event_types.clone(),
        categories: cfg.categories.clone(),
        rules: vec![], // External rule files not yet supported (W1-5 scope).
    }
}

/// Unimatrix knowledge engine.
#[derive(Parser)]
#[command(name = "unimatrix", about = "Unimatrix knowledge engine")]
struct Cli {
    /// Override project root directory.
    #[arg(long)]
    project_dir: Option<PathBuf>,

    /// Enable verbose logging.
    #[arg(long, short)]
    verbose: bool,

    /// Internal flag: set by run_daemon_launcher when spawning the daemon child.
    ///
    /// When present, main() calls prepare_daemon_child() (setsid) before entering
    /// the tokio runtime. Not intended for direct user invocation (R-17 / RV-03).
    #[arg(long, hide = true)]
    daemon_child: bool,

    /// Subcommand (hook, serve, stop, version, model-download, or none for bridge mode).
    #[command(subcommand)]
    command: Option<Command>,
}

/// Subcommands for the unimatrix binary.
#[derive(Debug, Subcommand)]
enum Command {
    /// Handle a Claude Code lifecycle hook event.
    ///
    /// Reads JSON from stdin, connects to the running server via UDS,
    /// and dispatches the event. No tokio runtime is initialized.
    Hook {
        /// The hook event name (e.g., SessionStart, Stop, Ping).
        event: String,
    },

    /// Export the knowledge base to JSONL format.
    ///
    /// Reads the database directly (no running server required) and writes
    /// all long-term knowledge to a portable JSONL file. Synchronous path,
    /// no tokio runtime.
    Export {
        /// Output file path. Defaults to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Import a knowledge base from a JSONL export file.
    ///
    /// Reads a nan-001 export dump, restores all 8 tables via direct SQL,
    /// re-embeds entries with the current ONNX model, and builds a fresh
    /// HNSW vector index. Synchronous path, no tokio runtime.
    Import {
        /// Input JSONL file path (required).
        #[arg(short, long)]
        input: PathBuf,

        /// Skip content hash and chain integrity validation.
        #[arg(long)]
        skip_hash_validation: bool,

        /// Drop all existing data before import.
        #[arg(long)]
        force: bool,
    },

    /// Print version and exit.
    ///
    /// Synchronous path, no tokio runtime.
    Version,

    /// Download the ONNX model to cache.
    ///
    /// With no flags: downloads the embedding model (existing behavior).
    /// With --nli: downloads the NLI cross-encoder model.
    /// With --nli --nli-model <name>: downloads the specified NLI model variant.
    /// Outputs the SHA-256 hash of the downloaded NLI ONNX file to stdout so the
    /// operator can pin it in config.toml under nli_model_sha256.
    ///
    /// Synchronous path, no tokio runtime.
    ModelDownload {
        /// Download the NLI cross-encoder model.
        ///
        /// When absent: download embedding model only (unchanged existing behavior).
        #[arg(long)]
        nli: bool,

        /// NLI model variant to download. Valid values: "minilm2", "minilm2-q8", "deberta", "deberta-q8".
        ///
        /// Only valid with --nli. Defaults to "minilm2-q8" when --nli is given without --nli-model.
        #[arg(long, requires = "nli")]
        nli_model: Option<String>,
    },

    /// Start the MCP server in daemon or stdio mode.
    ///
    /// `--daemon`: detach to background; use bridge mode (no args) for normal operation.
    /// `--stdio`: run in foreground stdio mode (pre-vnc-005 behavior; for development).
    Serve {
        /// Run as a detached background daemon.
        #[arg(long)]
        daemon: bool,

        /// Run in foreground stdio mode (pre-vnc-005 default behavior).
        #[arg(long)]
        stdio: bool,
    },

    /// Stop the running background daemon.
    ///
    /// Sends SIGTERM to the daemon (reads PID file) and waits up to 15 seconds
    /// for the process to exit. Synchronous path, no tokio runtime.
    ///
    /// Exit codes: 0 = stopped, 1 = no daemon / stale PID, 2 = timeout.
    Stop,

    /// Take a full-fidelity snapshot of the active database using VACUUM INTO.
    ///
    /// The snapshot is a self-contained SQLite file containing ALL tables.
    /// It is the input to `unimatrix eval scenarios` and `unimatrix eval run`.
    ///
    /// WARNING: The snapshot contains all database content including agent_id,
    /// session_id, and query history. Do not commit snapshots to version control
    /// or share outside your development environment. (NFR-07)
    ///
    /// The snapshot can be taken while the daemon is running. WAL-mode SQLite
    /// guarantees isolation: VACUUM INTO reads a consistent point-in-time snapshot.
    ///
    /// Synchronous path (pre-tokio). Uses block_export_sync internally for async sqlx.
    Snapshot {
        /// Output file path for the snapshot SQLite file (required).
        #[arg(long)]
        out: PathBuf,
    },

    /// Offline evaluation harness for Unimatrix intelligence changes.
    ///
    /// Subcommands: scenarios, run, report
    ///
    /// Use `unimatrix snapshot` first to produce a snapshot database, then:
    ///   unimatrix eval scenarios --db snap.db --out scenarios.jsonl
    ///   unimatrix eval run --db snap.db --scenarios scenarios.jsonl --configs a.toml --out results/
    ///   unimatrix eval report --results results/ --out report.md
    ///
    /// Memory note: each profile in `eval run` loads a separate vector index.
    /// For large snapshots (50k entries) with multiple profiles, ensure adequate RAM.
    /// Recommended: <= 2 candidate profiles on machines with 8 GB RAM.
    ///
    /// Synchronous path (pre-tokio). Async work uses block_export_sync internally.
    Eval {
        /// Eval subcommand (scenarios, run, report).
        #[command(subcommand)]
        command: unimatrix_server::eval::EvalCommand,
    },
}

/// Entry point: branches between sync subcommands and async paths.
///
/// ## Dispatch ordering (C-10)
///
/// 1. Hook and other sync subcommands dispatched FIRST — before any Tokio runtime.
/// 2. Stop dispatched SECOND — synchronous, no Tokio.
/// 3. daemon_child flag handled THIRD — setsid() before any runtime init (C-01).
/// 4. All async paths (serve --daemon child, serve --stdio, bridge) enter Tokio last.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install panic hook that logs to stderr before aborting (vnc-010).
    // Without this, panics in background tasks are swallowed silently.
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic payload".to_string()
        };
        let location = info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        eprintln!("PANIC at {location}: {payload}");
    }));

    let cli = Cli::parse();

    // C-10: Sync subcommands MUST be dispatched before any Tokio runtime init.
    // The match below runs in declaration order; all sync paths return here.
    match cli.command {
        Some(Command::Hook { event }) => {
            // Sync path: NO tokio, NO tracing init, NO database open
            // Minimal startup for <50ms budget
            return unimatrix_server::uds::hook::run(event, cli.project_dir);
        }
        Some(Command::Export { output }) => {
            // Sync path: NO tokio, like Hook
            return unimatrix_server::export::run_export(
                cli.project_dir.as_deref(),
                output.as_deref(),
            );
        }
        Some(Command::Import {
            input,
            skip_hash_validation,
            force,
        }) => {
            // Sync path: NO tokio, like Hook and Export
            return unimatrix_server::import::run_import(
                cli.project_dir.as_deref(),
                &input,
                skip_hash_validation,
                force,
            );
        }
        Some(Command::Version) => {
            // Sync path: NO tokio
            return handle_version(cli.project_dir);
        }
        Some(Command::ModelDownload { nli, nli_model }) => {
            // Sync path: NO tokio
            return handle_model_download(nli, nli_model);
        }
        Some(Command::Stop) => {
            // Sync path: NO tokio (ADR-006)
            // run_stop returns an exit code; we call std::process::exit here.
            let code = run_stop(cli.project_dir);
            std::process::exit(code);
        }
        Some(Command::Snapshot { out }) => {
            // Sync path: dispatched pre-tokio (C-09, C-10).
            // run_snapshot uses block_export_sync internally for async sqlx.
            return unimatrix_server::snapshot::run_snapshot(cli.project_dir.as_deref(), &out);
        }
        Some(Command::Eval { command: eval_cmd }) => {
            // Sync path: dispatched pre-tokio (C-10, ADR-005).
            // run_eval_command uses block_export_sync internally for async subcommands.
            return unimatrix_server::eval::run_eval_command(eval_cmd, cli.project_dir.as_deref());
        }
        Some(Command::Serve {
            daemon: true,
            stdio: _,
        }) => {
            // Daemon path — launcher or child (ADR-001 / C-01).
            //
            // If --daemon-child is set: we are the spawned child process.
            //   C-01: prepare_daemon_child() (setsid) MUST be called before
            //   tokio_main_daemon() initializes the Tokio runtime.
            // If --daemon-child is NOT set: we are the launcher.
            //   Run synchronously: spawn child + poll socket.
            if cli.daemon_child {
                // C-01: setsid() before Tokio runtime init.
                unimatrix_server::infra::daemon::prepare_daemon_child()?;
                // Fall through to tokio_main_daemon below.
                return tokio_main_daemon(cli);
            } else {
                // Launcher path: synchronous spawn + poll for socket.
                let paths = compute_paths_sync(&cli.project_dir)?;
                unimatrix_server::infra::daemon::run_daemon_launcher(&paths)?;
                return Ok(());
            }
        }
        Some(Command::Serve {
            daemon: false,
            stdio: _,
        }) => {
            // Stdio mode: serve --stdio or bare `serve` with no flags.
            // Identical to pre-vnc-005 default behavior (R-12 regression gate).
            return tokio_main_stdio(cli);
        }
        None => {
            // No subcommand: bridge mode (vnc-005 default invocation).
            // C-10: only reached after all sync dispatch arms above.
            if cli.daemon_child {
                // --daemon-child with no subcommand: should not happen in normal
                // operation but handle defensively — fall into daemon path.
                unimatrix_server::infra::daemon::prepare_daemon_child()?;
                return tokio_main_daemon(cli);
            }
            return tokio_main_bridge(cli);
        }
    }
}

/// Resolve project paths synchronously without initializing any async runtime.
///
/// Used in the launcher path where only paths are needed (no server init).
fn compute_paths_sync(
    project_dir: &Option<PathBuf>,
) -> Result<unimatrix_engine::project::ProjectPaths, Box<dyn std::error::Error>> {
    project::ensure_data_directory(project_dir.as_deref(), None).map_err(|e| {
        Box::new(ServerError::ProjectInit(e.to_string())) as Box<dyn std::error::Error>
    })
}

/// Synchronous stop subcommand (no Tokio runtime).
///
/// Reads PID file, verifies the process via `is_unimatrix_process`, sends SIGTERM
/// via `terminate_and_wait`, and polls for exit.
///
/// ## Exit codes (ADR-006)
///
/// - 0: daemon stopped successfully
/// - 1: no daemon running, no PID file, or stale PID
/// - 2: daemon did not exit within 15-second timeout
fn run_stop(project_dir: Option<PathBuf>) -> i32 {
    // Step 1: Resolve project paths (same as hook path — synchronous).
    let paths = match project::ensure_data_directory(project_dir.as_deref(), None) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: failed to resolve project paths: {e}");
            return 1;
        }
    };

    // Step 2: Read PID file.
    let pid = match pidfile::read_pid_file(&paths.pid_path) {
        Some(p) => p,
        None => {
            eprintln!("no unimatrix daemon running for this project (no PID file)");
            return 1;
        }
    };

    // Step 3: Verify it is a unimatrix process.
    // On macOS/BSD: falls back to is_process_alive (no /proc — existing limitation).
    if !pidfile::is_unimatrix_process(pid) {
        eprintln!("stale PID file: process {pid} is not a unimatrix daemon (or has exited)",);
        return 1;
    }

    // Step 4: Send SIGTERM and wait (ADR-006: 15s to accommodate graceful shutdown).
    let stopped = pidfile::terminate_and_wait(pid, Duration::from_secs(15));

    // Step 5: Report result.
    if stopped {
        println!("unimatrix daemon stopped (PID {pid})");
        0 // exit code 0: daemon stopped
    } else {
        eprintln!("daemon (PID {pid}) did not stop within 15 seconds");
        2 // exit code 2: timeout (ADR-006 exit code specification)
    }
}

// ---------------------------------------------------------------------------
// Async entry points
// ---------------------------------------------------------------------------

/// Async entry point for daemon child mode (new in vnc-005).
///
/// Called after `prepare_daemon_child()` has run setsid(). Starts the full
/// MCP server stack with a UDS acceptor; waits for the daemon token (SIGTERM/SIGINT).
///
/// C-04: There is exactly ONE call to `graceful_shutdown` in this codebase,
/// reachable only from the daemon token cancellation path below.
#[tokio::main]
async fn tokio_main_daemon(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing (daemon: log to stderr, redirected to log file by launcher).
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("starting unimatrix daemon");

    // Initialize project paths.
    let paths = project::ensure_data_directory(cli.project_dir.as_deref(), None)
        .map_err(|e| ServerError::ProjectInit(e.to_string()))?;

    tracing::info!(
        project_root = %paths.project_root.display(),
        project_hash = %paths.project_hash,
        data_dir = %paths.data_dir.display(),
        mcp_socket = %paths.mcp_socket_path.display(),
        "daemon project initialized"
    );

    // ── dsn-001: Load external config ─────────────────────────────────────────────
    // dirs::home_dir() returns None in rootless/container environments.
    // When None: log a warning and proceed with compiled defaults (R-15).
    let config = match dirs::home_dir() {
        Some(home) => match load_config(&home, &paths.data_dir) {
            Ok(cfg) => {
                tracing::info!(preset = ?cfg.profile.preset, "config loaded");
                cfg
            }
            Err(e) => {
                tracing::warn!(error = %e, "config load failed; using compiled defaults");
                UnimatrixConfig::default()
            }
        },
        None => {
            tracing::warn!("home directory not found; using compiled defaults (R-15)");
            UnimatrixConfig::default()
        }
    };

    // Resolve ConfidenceParams from preset/weights.
    let confidence_params = Arc::new(resolve_confidence_params(&config).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "confidence params resolution failed; using defaults");
        ConfidenceParams::default()
    }));

    // Extract concrete values for subsystem constructors.
    // None of these are stored as Arc<UnimatrixConfig> on any struct (ADR-002).
    let knowledge_categories: Vec<String> = config.knowledge.categories.clone();
    let boosted_categories: HashSet<String> = config
        .knowledge
        .boosted_categories
        .iter()
        .cloned()
        .collect();
    let server_instructions: Option<String> = config.server.instructions.clone();
    let permissive: bool = config.agents.default_trust == "permissive";
    let session_caps: Vec<Capability> = config
        .agents
        .session_capabilities
        .iter()
        .filter_map(|s| match s.as_str() {
            "Read" => Some(Capability::Read),
            "Write" => Some(Capability::Write),
            "Search" => Some(Capability::Search),
            _ => None, // unreachable: validate_config guards this
        })
        .collect();
    // ── end dsn-001 config load ────────────────────────────────────────────────────

    // Handle stale PID file before attempting to open the database.
    match pidfile::handle_stale_pid_file(&paths.pid_path, STALE_PROCESS_TIMEOUT) {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!("stale process did not exit; will attempt database open anyway");
        }
        Err(e) => {
            tracing::warn!(error = %e, "PID file handling failed; continuing startup");
        }
    }

    // Open database with retry loop for lock contention.
    let store = open_store_with_retry(&paths.db_path).await?;

    // Acquire PID guard (flock + write PID).
    let _pid_guard = match pidfile::PidGuard::acquire(&paths.pid_path) {
        Ok(guard) => {
            tracing::info!("PID guard acquired");
            Some(guard)
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to acquire PID guard; continuing without it");
            None
        }
    };

    // Handle stale hook IPC socket file (unconditional unlink per ADR-004).
    uds_listener::handle_stale_socket(&paths.socket_path)?;

    // Initialize vector index.
    let vector_config = VectorConfig::default();
    let meta_path = paths.vector_dir.join("unimatrix-vector.meta");

    let vector_index = if meta_path.exists() {
        tracing::info!("loading existing vector index");
        Arc::new(
            VectorIndex::load(Arc::clone(&store), vector_config, &paths.vector_dir)
                .await
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?,
        )
    } else {
        tracing::info!("creating new vector index");
        Arc::new(
            VectorIndex::new(Arc::clone(&store), vector_config)
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?,
        )
    };

    // Initialize embedding service (lazy — background task).
    let embed_handle = EmbedServiceHandle::new();
    embed_handle.start_loading(EmbedConfig::default());

    // Initialize agent registry and bootstrap defaults.
    let registry = Arc::new(AgentRegistry::new(
        Arc::clone(&store),
        permissive,
        session_caps,
    )?);
    registry.bootstrap_defaults()?;

    // Initialize audit log.
    let audit = Arc::new(AuditLog::new(Arc::clone(&store)));

    // Build adapters and async wrappers.
    let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));

    let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

    // Initialize category allowlist from config (dsn-001).
    let categories = Arc::new(CategoryAllowlist::from_categories(knowledge_categories));

    // Build DomainPackRegistry from [observation] config (col-023 ADR-002).
    // The built-in claude-code pack is always loaded; TOML stanzas are merged in.
    let _observation_registry = {
        let packs: Vec<DomainPack> = config
            .observation
            .domain_packs
            .iter()
            .map(domain_pack_from_config)
            .collect();
        let reg = DomainPackRegistry::new(packs).map_err(|e| {
            ServerError::ProjectInit(format!("domain pack registry init failed: {e}"))
        })?;
        // Register domain pack categories into CategoryAllowlist (IR-02 ordering).
        for pack in reg.iter_packs() {
            for category in &pack.categories {
                categories.add_category(category.clone());
            }
        }
        Arc::new(reg)
    };

    // Initialize adaptation service (crt-006).
    let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));
    if let Err(e) = adapt_service.load_state(&paths.data_dir) {
        tracing::warn!("adaptation state load failed: {e}, starting fresh");
    } else {
        let training_gen = adapt_service.training_generation();
        if training_gen > 0 {
            tracing::info!(generation = training_gen, "adaptation state restored");
        }
    }

    // Create session registry for hook IPC (col-008).
    let session_registry = Arc::new(unimatrix_server::infra::session::SessionRegistry::new());

    // Create pending entries analysis accumulator (col-009).
    let pending_entries_analysis = Arc::new(Mutex::new(PendingEntriesAnalysis::new()));

    // ── crt-022: Initialize rayon ML inference pool (ADR-004) ────────────────────────
    // Validate rayon_pool_size is in [1, 64] before constructing the pool.
    // InferenceConfig::validate() returns ConfigError on out-of-range — map to startup error.
    config
        .inference
        .validate(&paths.data_dir.join("config.toml"))
        .map_err(|e| ServerError::InferencePoolInit(e.to_string()))?;

    // crt-023 (ADR-001): apply NLI pool floor — when nli_enabled, pool must be >= 6 (max 8).
    let effective_pool_size = if config.inference.nli_enabled {
        config.inference.rayon_pool_size.max(6).min(8)
    } else {
        config.inference.rayon_pool_size
    };

    let ml_inference_pool = Arc::new(
        RayonPool::new(effective_pool_size, "ml_inference_pool")
            .map_err(|e| ServerError::InferencePoolInit(e.to_string()))?,
    );
    tracing::info!(
        pool_size = effective_pool_size,
        nli_enabled = config.inference.nli_enabled,
        "ml_inference_pool initialized"
    );
    // TODO(W2-4): add gguf_rayon_pool: Arc<RayonPool> here
    // ── end crt-022 ───────────────────────────────────────────────────────────────────

    // crt-023: Build NLI service handle and start loading when enabled.
    let nli_handle = NliServiceHandle::new();
    if config.inference.nli_enabled {
        let cache_dir = unimatrix_embed::EmbedConfig::default().resolve_cache_dir();
        let nli_config = NliConfig {
            nli_enabled: true,
            nli_model_name: config.inference.nli_model_name.clone(),
            nli_model_path: config.inference.nli_model_path.clone(),
            nli_model_sha256: config.inference.nli_model_sha256.clone(),
            cache_dir,
        };
        nli_handle.start_loading(nli_config);
    } else {
        tracing::info!(
            "NLI cross-encoder disabled (nli_enabled=false); search uses cosine fallback"
        );
    }

    // Build shared ServiceLayer for UDS and MCP transports (vnc-006, vnc-009).
    let usage_dedup = Arc::new(UsageDedup::new());
    let inference_config = Arc::new(config.inference.clone()); // crt-023: snapshot for service layer
    let services = unimatrix_server::services::ServiceLayer::new(
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&async_vector_store),
        Arc::clone(&store),
        Arc::clone(&embed_handle),
        Arc::clone(&adapt_service),
        Arc::clone(&audit),
        Arc::clone(&usage_dedup),
        boosted_categories,
        Arc::clone(&ml_inference_pool),
        Arc::clone(&nli_handle), // clone: nli_handle also needed by spawn_background_tick
        config.inference.nli_top_k,
        config.inference.nli_enabled,
        Arc::clone(&inference_config), // crt-023: NLI store config snapshot
    );

    // Start UDS listener for hook IPC.
    let server_uid = nix::unistd::getuid().as_raw();
    let (uds_handle, socket_guard) = uds_listener::start_uds_listener(
        &paths.socket_path,
        Arc::clone(&store),
        Arc::clone(&embed_handle),
        Arc::clone(&async_vector_store),
        Arc::clone(&store),
        Arc::clone(&adapt_service),
        Arc::clone(&session_registry),
        Arc::clone(&pending_entries_analysis),
        server_uid,
        env!("CARGO_PKG_VERSION").to_string(),
        services.clone(),
        Arc::clone(&audit),
    )
    .await?;

    // Build server (ADR-003: constructed once, cloned into each session task).
    let mut server = UnimatrixServer::new(
        Arc::clone(&store),
        async_vector_store,
        Arc::clone(&embed_handle),
        Arc::clone(&registry),
        Arc::clone(&audit),
        categories,
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&adapt_service),
        server_instructions,
    );
    // Share pending_entries_analysis and session_registry with the MCP server (col-009).
    server.pending_entries_analysis = Arc::clone(&pending_entries_analysis);
    server.session_registry = Arc::clone(&session_registry);

    // Extract state handles before services is moved.
    let confidence_state_handle = services.confidence_state_handle();
    let effectiveness_state_handle = services.effectiveness_state_handle();
    let typed_graph_handle = services.typed_graph_handle();
    let contradiction_cache_handle = services.contradiction_cache_handle();

    // Parse auto-quarantine threshold at startup (Constraint 14).
    let auto_quarantine_cycles = unimatrix_server::background::parse_auto_quarantine_cycles()
        .map_err(ServerError::ProjectInit)?;

    // Spawn background tick for automated maintenance + extraction (col-013).
    let tick_handle = unimatrix_server::background::spawn_background_tick(
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&embed_handle),
        Arc::clone(&adapt_service),
        Arc::clone(&session_registry),
        Arc::clone(&store),
        Arc::clone(&pending_entries_analysis),
        Arc::clone(&server.tick_metadata),
        None,
        confidence_state_handle,
        effectiveness_state_handle,
        typed_graph_handle,
        contradiction_cache_handle,
        Arc::clone(&audit),
        auto_quarantine_cycles,
        Arc::clone(&confidence_params),
        Arc::clone(&ml_inference_pool),
        config.inference.nli_enabled, // crt-023 (ADR-007)
        config.inference.nli_auto_quarantine_threshold, // crt-023 (ADR-007)
        nli_handle,                   // crt-023: bootstrap promotion
        Arc::clone(&inference_config), // crt-023: bootstrap promotion config
    );

    // Create daemon CancellationToken (ADR-002).
    let daemon_token = shutdown::new_daemon_token();

    // Start MCP UDS acceptor (vnc-005: Component 2).
    let (mcp_acceptor_handle, mcp_socket_guard) =
        unimatrix_server::uds::mcp_listener::start_mcp_uds_listener(
            &paths.mcp_socket_path,
            server.clone(),
            daemon_token.clone(),
        )
        .await?;

    // Signal handler: cancel daemon token on SIGTERM/SIGINT.
    // This is the ONLY path that triggers graceful_shutdown (C-04 / C-05).
    let signal_token = daemon_token.clone();
    tokio::spawn(async move {
        shutdown::shutdown_signal().await;
        tracing::info!("received shutdown signal; cancelling daemon token");
        signal_token.cancel();
    });

    // Build LifecycleHandles with new vnc-005 fields.
    // Drop ordering is enforced by graceful_shutdown (see infra/shutdown.rs).
    let lifecycle_handles = LifecycleHandles {
        store,
        vector_index,
        vector_dir: paths.vector_dir.clone(),
        registry,
        audit,
        adapt_service,
        data_dir: paths.data_dir.clone(),
        mcp_socket_guard: Some(mcp_socket_guard), // vnc-005: MCP UDS socket
        mcp_acceptor_handle: Some(mcp_acceptor_handle), // vnc-005: MCP accept loop
        socket_guard: Some(socket_guard),
        uds_handle: Some(uds_handle),
        tick_handle: Some(tick_handle),
        services: Some(services),
    };

    tracing::info!("unimatrix daemon ready");

    // Wait for daemon token cancellation (SIGTERM/SIGINT via signal handler above).
    //
    // ADR-002 / C-04: Session EOF (QuitReason::Closed) does NOT cancel this token.
    // The daemon survives individual session disconnections.
    daemon_token.cancelled().await;
    tracing::info!("daemon token cancelled; beginning graceful shutdown");

    // C-05: ONLY call site for graceful_shutdown in the daemon path.
    // The serve --stdio path uses its own QuitReason::Closed → graceful_shutdown path below.
    shutdown::graceful_shutdown(lifecycle_handles).await?;

    tracing::info!("unimatrix daemon exited cleanly");
    Ok(())
}

/// Async entry point for stdio mode (refactored from the pre-vnc-005 `tokio_main`).
///
/// Identical to the pre-vnc-005 default behavior: serves MCP over stdio,
/// exits when stdin closes (R-12 regression gate) or on signal.
///
/// The new `mcp_socket_guard: None` and `mcp_acceptor_handle: None` fields are
/// supplied to satisfy the updated `LifecycleHandles` struct.
#[tokio::main]
async fn tokio_main_stdio(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing (logs to stderr — stdout is for MCP protocol).
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("starting unimatrix server");

    // Initialize project data directory.
    let paths = project::ensure_data_directory(cli.project_dir.as_deref(), None)
        .map_err(|e| ServerError::ProjectInit(e.to_string()))?;

    tracing::info!(
        project_root = %paths.project_root.display(),
        project_hash = %paths.project_hash,
        data_dir = %paths.data_dir.display(),
        "project initialized"
    );

    // ── dsn-001: Load external config ─────────────────────────────────────────────
    // dirs::home_dir() returns None in rootless/container environments.
    // When None: log a warning and proceed with compiled defaults (R-15).
    let config = match dirs::home_dir() {
        Some(home) => match load_config(&home, &paths.data_dir) {
            Ok(cfg) => {
                tracing::info!(preset = ?cfg.profile.preset, "config loaded");
                cfg
            }
            Err(e) => {
                tracing::warn!(error = %e, "config load failed; using compiled defaults");
                UnimatrixConfig::default()
            }
        },
        None => {
            tracing::warn!("home directory not found; using compiled defaults (R-15)");
            UnimatrixConfig::default()
        }
    };

    // Resolve ConfidenceParams from preset/weights.
    let confidence_params = Arc::new(resolve_confidence_params(&config).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "confidence params resolution failed; using defaults");
        ConfidenceParams::default()
    }));

    // Extract concrete values for subsystem constructors.
    // None of these are stored as Arc<UnimatrixConfig> on any struct (ADR-002).
    let knowledge_categories: Vec<String> = config.knowledge.categories.clone();
    let boosted_categories: HashSet<String> = config
        .knowledge
        .boosted_categories
        .iter()
        .cloned()
        .collect();
    let server_instructions: Option<String> = config.server.instructions.clone();
    let permissive: bool = config.agents.default_trust == "permissive";
    let session_caps: Vec<Capability> = config
        .agents
        .session_capabilities
        .iter()
        .filter_map(|s| match s.as_str() {
            "Read" => Some(Capability::Read),
            "Write" => Some(Capability::Write),
            "Search" => Some(Capability::Search),
            _ => None, // unreachable: validate_config guards this
        })
        .collect();
    // ── end dsn-001 config load ────────────────────────────────────────────────────

    // Handle stale PID file before attempting to open the database.
    match pidfile::handle_stale_pid_file(&paths.pid_path, STALE_PROCESS_TIMEOUT) {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!("stale process did not exit; will attempt database open anyway");
        }
        Err(e) => {
            tracing::warn!(error = %e, "PID file handling failed; continuing startup");
        }
    }

    // Open database with retry loop for lock contention.
    let store = open_store_with_retry(&paths.db_path).await?;

    // Acquire PID guard (flock + write PID) now that we hold the database lock.
    // PidGuard::drop will remove the PID file and release the lock on exit.
    let _pid_guard = match pidfile::PidGuard::acquire(&paths.pid_path) {
        Ok(guard) => {
            tracing::info!("PID guard acquired");
            Some(guard)
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to acquire PID guard; continuing without it");
            None
        }
    };

    // Handle stale socket file (unconditional unlink per ADR-004).
    uds_listener::handle_stale_socket(&paths.socket_path)?;

    // Initialize vector index.
    let vector_config = VectorConfig::default();
    let meta_path = paths.vector_dir.join("unimatrix-vector.meta");

    let vector_index = if meta_path.exists() {
        tracing::info!("loading existing vector index");
        Arc::new(
            VectorIndex::load(Arc::clone(&store), vector_config, &paths.vector_dir)
                .await
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?,
        )
    } else {
        tracing::info!("creating new vector index");
        Arc::new(
            VectorIndex::new(Arc::clone(&store), vector_config)
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?,
        )
    };

    // Initialize embedding service (lazy — background task).
    let embed_handle = EmbedServiceHandle::new();
    embed_handle.start_loading(EmbedConfig::default());

    // Initialize agent registry and bootstrap defaults.
    let registry = Arc::new(AgentRegistry::new(
        Arc::clone(&store),
        permissive,
        session_caps,
    )?);
    registry.bootstrap_defaults()?;

    // Initialize audit log.
    let audit = Arc::new(AuditLog::new(Arc::clone(&store)));

    // Build adapters and async wrappers.
    let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));

    let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

    // Initialize category allowlist from config (dsn-001).
    let categories = Arc::new(CategoryAllowlist::from_categories(knowledge_categories));

    // Build DomainPackRegistry from [observation] config (col-023 ADR-002).
    // The built-in claude-code pack is always loaded; TOML stanzas are merged in.
    let _observation_registry = {
        let packs: Vec<DomainPack> = config
            .observation
            .domain_packs
            .iter()
            .map(domain_pack_from_config)
            .collect();
        let reg = DomainPackRegistry::new(packs).map_err(|e| {
            ServerError::ProjectInit(format!("domain pack registry init failed: {e}"))
        })?;
        // Register domain pack categories into CategoryAllowlist (IR-02 ordering).
        for pack in reg.iter_packs() {
            for category in &pack.categories {
                categories.add_category(category.clone());
            }
        }
        Arc::new(reg)
    };

    // Initialize adaptation service (crt-006).
    let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));
    if let Err(e) = adapt_service.load_state(&paths.data_dir) {
        tracing::warn!("adaptation state load failed: {e}, starting fresh");
    } else {
        let training_gen = adapt_service.training_generation();
        if training_gen > 0 {
            tracing::info!(generation = training_gen, "adaptation state restored");
        }
    }

    // Create session registry for hook IPC (col-008).
    let session_registry = Arc::new(unimatrix_server::infra::session::SessionRegistry::new());

    // Create pending entries analysis accumulator shared between UDS listener and MCP server (col-009).
    let pending_entries_analysis = Arc::new(Mutex::new(PendingEntriesAnalysis::new()));

    // ── crt-022: Initialize rayon ML inference pool (ADR-004) ────────────────────────
    // Validate rayon_pool_size is in [1, 64] before constructing the pool.
    config
        .inference
        .validate(&paths.data_dir.join("config.toml"))
        .map_err(|e| ServerError::InferencePoolInit(e.to_string()))?;

    // crt-023 (ADR-001): apply NLI pool floor — when nli_enabled, pool must be >= 6 (max 8).
    let effective_pool_size = if config.inference.nli_enabled {
        config.inference.rayon_pool_size.max(6).min(8)
    } else {
        config.inference.rayon_pool_size
    };

    let ml_inference_pool = Arc::new(
        RayonPool::new(effective_pool_size, "ml_inference_pool")
            .map_err(|e| ServerError::InferencePoolInit(e.to_string()))?,
    );
    tracing::info!(
        pool_size = effective_pool_size,
        nli_enabled = config.inference.nli_enabled,
        "ml_inference_pool initialized"
    );
    // TODO(W2-4): add gguf_rayon_pool: Arc<RayonPool> here
    // ── end crt-022 ───────────────────────────────────────────────────────────────────

    // crt-023: Build NLI service handle and start loading when enabled.
    let nli_handle = NliServiceHandle::new();
    if config.inference.nli_enabled {
        let cache_dir = unimatrix_embed::EmbedConfig::default().resolve_cache_dir();
        let nli_config = NliConfig {
            nli_enabled: true,
            nli_model_name: config.inference.nli_model_name.clone(),
            nli_model_path: config.inference.nli_model_path.clone(),
            nli_model_sha256: config.inference.nli_model_sha256.clone(),
            cache_dir,
        };
        nli_handle.start_loading(nli_config);
    } else {
        tracing::info!(
            "NLI cross-encoder disabled (nli_enabled=false); search uses cosine fallback"
        );
    }

    // Build shared ServiceLayer for UDS and MCP transports (vnc-006, vnc-009).
    let usage_dedup = Arc::new(UsageDedup::new());
    let inference_config = Arc::new(config.inference.clone()); // crt-023: snapshot for service layer
    let services = unimatrix_server::services::ServiceLayer::new(
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&async_vector_store),
        Arc::clone(&store),
        Arc::clone(&embed_handle),
        Arc::clone(&adapt_service),
        Arc::clone(&audit),
        Arc::clone(&usage_dedup),
        boosted_categories,
        Arc::clone(&ml_inference_pool),
        Arc::clone(&nli_handle), // clone: nli_handle also needed by spawn_background_tick
        config.inference.nli_top_k,
        config.inference.nli_enabled,
        Arc::clone(&inference_config), // crt-023: NLI store config snapshot
    );

    // Start UDS listener for hook IPC (expanded signature per col-007 ADR-001, col-008, col-009).
    let server_uid = nix::unistd::getuid().as_raw();
    let (uds_handle, socket_guard) = uds_listener::start_uds_listener(
        &paths.socket_path,
        Arc::clone(&store),
        Arc::clone(&embed_handle),
        Arc::clone(&async_vector_store),
        Arc::clone(&store),
        Arc::clone(&adapt_service),
        Arc::clone(&session_registry),
        Arc::clone(&pending_entries_analysis),
        server_uid,
        env!("CARGO_PKG_VERSION").to_string(),
        services.clone(),
        Arc::clone(&audit),
    )
    .await?;

    // Build server.
    let mut server = UnimatrixServer::new(
        Arc::clone(&store),
        async_vector_store,
        Arc::clone(&embed_handle),
        Arc::clone(&registry),
        Arc::clone(&audit),
        categories,
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&adapt_service),
        server_instructions,
    );
    // Share pending_entries_analysis and session_registry with the MCP server (col-009).
    server.pending_entries_analysis = Arc::clone(&pending_entries_analysis);
    server.session_registry = Arc::clone(&session_registry);

    // crt-019: extract ConfidenceStateHandle before services is moved.
    let confidence_state_handle = services.confidence_state_handle();
    // crt-018b: extract EffectivenessStateHandle before services is moved.
    let effectiveness_state_handle = services.effectiveness_state_handle();
    // crt-021: extract TypedGraphStateHandle before services is moved.
    let typed_graph_handle = services.typed_graph_handle();
    // GH #278: extract ContradictionScanCacheHandle before services is moved.
    let contradiction_cache_handle = services.contradiction_cache_handle();

    // crt-018b: parse auto-quarantine threshold at startup (Constraint 14).
    let auto_quarantine_cycles = unimatrix_server::background::parse_auto_quarantine_cycles()
        .map_err(ServerError::ProjectInit)?;

    // Spawn background tick for automated maintenance + extraction (col-013).
    let tick_handle = unimatrix_server::background::spawn_background_tick(
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&embed_handle),
        Arc::clone(&adapt_service),
        Arc::clone(&session_registry),
        Arc::clone(&store),
        Arc::clone(&pending_entries_analysis),
        Arc::clone(&server.tick_metadata),
        None, // TrainingService: wired in future integration step
        confidence_state_handle,
        effectiveness_state_handle, // crt-018b: shared with search/briefing paths
        typed_graph_handle,         // crt-021: shared with SearchService
        contradiction_cache_handle, // GH #278: shared with StatusService
        Arc::clone(&audit),         // crt-018b: for tick_skipped audit events
        auto_quarantine_cycles,     // crt-018b: auto-quarantine threshold
        Arc::clone(&confidence_params),
        Arc::clone(&ml_inference_pool), // crt-022 (ADR-004): ML inference pool
        config.inference.nli_enabled,   // crt-023 (ADR-007)
        config.inference.nli_auto_quarantine_threshold, // crt-023 (ADR-007)
        nli_handle,                     // crt-023: bootstrap promotion
        Arc::clone(&inference_config),  // crt-023: bootstrap promotion config
    );

    // Prepare lifecycle handles for shutdown.
    // ServiceLayer is moved here so graceful_shutdown can drop it before
    // Arc::try_unwrap(store), releasing its internal Arc<Store> clones (#92).
    let lifecycle_handles = LifecycleHandles {
        store,
        vector_index,
        vector_dir: paths.vector_dir.clone(),
        registry,
        audit,
        adapt_service,
        data_dir: paths.data_dir.clone(),
        // vnc-005: stdio mode does not use MCP UDS socket or acceptor task
        mcp_socket_guard: None,
        mcp_acceptor_handle: None,
        socket_guard: Some(socket_guard),
        uds_handle: Some(uds_handle),
        tick_handle: Some(tick_handle),
        services: Some(services),
    };

    // Serve over stdio (R-12 regression gate: must exit when stdin closes).
    tracing::info!("serving MCP over stdio");
    let running = server
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| ServerError::Shutdown(e.to_string()))?;

    // Register signal handler to cancel the transport on SIGTERM/SIGINT (#236).
    //
    // Previous approach passed a `waiting()` future to `graceful_shutdown` and
    // used `tokio::select!` to race against the signal. On signal, the `waiting()`
    // future was dropped, which dropped the RunningService. The Drop impl
    // triggers async cancellation via a DropGuard, but by then the tokio runtime
    // is shutting down and the blocking stdin reader is never closed -- ghost process.
    //
    // New approach: get the cancellation token BEFORE calling `waiting()`.
    // A spawned task monitors the shutdown signal and cancels the token, which
    // causes the rmcp service loop to exit and close the transport properly.
    // `waiting()` then returns normally with `QuitReason::Cancelled`.
    let cancel_token = running.cancellation_token();
    tokio::spawn(async move {
        shutdown::shutdown_signal().await;
        tracing::info!("received shutdown signal, cancelling MCP transport");
        cancel_token.cancel();
    });

    // Wait for the service to complete. This returns when either:
    // - The transport closes naturally (client disconnect) -> QuitReason::Closed
    // - The signal handler cancels the token -> QuitReason::Cancelled
    //
    // R-12 regression gate: QuitReason::Closed (stdin EOF) must reach graceful_shutdown.
    match running.waiting().await {
        Ok(reason) => tracing::info!(?reason, "MCP transport closed"),
        Err(e) => tracing::error!(error = %e, "MCP transport task failed"),
    }

    // Run lifecycle shutdown (vector dump, adapt save, DB compaction).
    // C-04: This is the graceful_shutdown call for stdio mode (separate from daemon path).
    shutdown::graceful_shutdown(lifecycle_handles).await?;

    tracing::info!("unimatrix server exited cleanly");
    Ok(())
}

/// Async entry point for bridge mode (new default no-subcommand path in vnc-005).
///
/// Connects Claude Code's stdio pipe to the running daemon's MCP UDS socket,
/// auto-starting the daemon if absent. The bridge carries no Unimatrix
/// capabilities (C-06); all auth enforcement lives in the daemon.
#[tokio::main]
async fn tokio_main_bridge(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing. Bridge logs go to stderr (stdout is for MCP protocol).
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    tracing::debug!("bridge mode starting");

    // Resolve project paths (needed for mcp_socket_path, pid_path, log_path).
    let paths = project::ensure_data_directory(cli.project_dir.as_deref(), None)
        .map_err(|e| ServerError::ProjectInit(e.to_string()))?;

    tracing::debug!(
        mcp_socket = %paths.mcp_socket_path.display(),
        "bridge connecting to daemon"
    );

    // Delegate to the bridge module (Component 6).
    unimatrix_server::bridge::run_bridge(&paths).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Print version string to stdout and exit.
///
/// When `--project-dir` is provided, also pre-creates the data directory and
/// database (used by `npx unimatrix init` to ensure DB exists before first run).
fn handle_version(project_dir: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(dir) = project_dir {
        let paths = project::ensure_data_directory(Some(&dir), None)
            .map_err(|e| ServerError::ProjectInit(e.to_string()))?;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let _store = rt
            .block_on(Store::open(
                &paths.db_path,
                unimatrix_store::PoolConfig::default(),
            ))
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
        eprintln!("database initialized at {}", paths.db_path.display());
    }

    println!("unimatrix {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

/// Download ONNX model(s) to cache (crt-023: extended with --nli / --nli-model, AC-16).
///
/// When `nli = false` (default): downloads embedding model only (unchanged behavior).
/// When `nli = true`: downloads embedding model AND the specified NLI model, then
///   computes and prints the SHA-256 hash of the NLI ONNX file to stdout.
///
/// Progress messages go to stderr; hash output goes to stdout (operator captures via pipe).
fn handle_model_download(
    nli: bool,
    nli_model_name: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Download embedding model (unchanged existing behavior).
    let embed_config = EmbedConfig::default();
    let cache_dir = embed_config.resolve_cache_dir();

    eprintln!(
        "Downloading ONNX embedding model to {}...",
        cache_dir.display()
    );

    match unimatrix_embed::ensure_model(embed_config.model, &cache_dir) {
        Ok(model_dir) => eprintln!("Embedding model ready: {}", model_dir.display()),
        Err(e) => {
            eprintln!("Embedding model download failed: {e}");
            return Err(Box::new(e));
        }
    }

    // Step 2: If --nli flag not given, return here (existing behavior preserved).
    if !nli {
        return Ok(());
    }

    // Step 3: Resolve the NLI model variant.
    let nli_model: NliModel = {
        let name = nli_model_name.as_deref().unwrap_or("minilm2-q8");
        NliModel::from_config_name(name).ok_or_else(|| {
            eprintln!(
                "Error: unrecognized --nli-model value '{}'; valid: minilm2, minilm2-q8, deberta, deberta-q8",
                name
            );
            format!("unrecognized nli-model: {name}")
        })?
    };

    // Step 4: Download the NLI model via ensure_nli_model (mirrors ensure_model pattern).
    eprintln!(
        "Downloading NLI model '{}' to {}...",
        nli_model.model_id(),
        cache_dir.display()
    );

    let model_dir = match unimatrix_embed::ensure_nli_model(nli_model, &cache_dir) {
        Ok(dir) => {
            eprintln!("NLI model ready: {}", dir.display());
            dir
        }
        Err(e) => {
            eprintln!("NLI model download failed: {e}");
            return Err(Box::new(e));
        }
    };

    // Step 5: Compute SHA-256 hash of the ONNX file.
    let onnx_path = model_dir.join(nli_model.onnx_filename());
    eprintln!("Computing SHA-256 hash of {}...", onnx_path.display());
    let hash_hex = compute_file_sha256(&onnx_path)?;

    // Step 6: Print hash to stdout (operator copies to config.toml).
    // Format: one line, lowercase hex, 64 chars. Ready to paste.
    println!("{}", hash_hex);

    // Step 7: Print guidance to stderr.
    eprintln!();
    eprintln!("Add the following to your config.toml under [inference]:");
    eprintln!("  nli_model_sha256 = \"{}\"", hash_hex);

    Ok(())
}

/// Compute the SHA-256 hash of a file and return as a lowercase hex string (64 chars).
///
/// Reads the file in 64 KB chunks to avoid loading the entire model into memory.
fn compute_file_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("failed to open {}: {e}", path.display()))?;

    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Maximum number of database open attempts before giving up.
const DB_OPEN_MAX_ATTEMPTS: u32 = 3;

/// Base delay between database open retries (doubles each attempt).
const DB_OPEN_RETRY_BASE_MS: u64 = 1000;

/// Open the database store with retry on lock contention.
///
/// Retries up to `DB_OPEN_MAX_ATTEMPTS` times with exponential backoff
/// (1s, 2s, 4s). This gives a stale process time to release the SQLite
/// lock after receiving SIGTERM in `handle_stale_pid_file` (#146).
async fn open_store_with_retry(
    db_path: &std::path::Path,
) -> Result<Arc<Store>, Box<dyn std::error::Error>> {
    let mut last_err = None;
    for attempt in 1..=DB_OPEN_MAX_ATTEMPTS {
        match Store::open(db_path, unimatrix_store::PoolConfig::default()).await {
            Ok(store) => {
                if attempt > 1 {
                    tracing::info!(attempt, "database opened after retry");
                }
                return Ok(Arc::new(store));
            }
            Err(e) => {
                if attempt < DB_OPEN_MAX_ATTEMPTS {
                    let delay_ms = DB_OPEN_RETRY_BASE_MS * 2u64.pow(attempt - 1);
                    tracing::warn!(
                        attempt,
                        max_attempts = DB_OPEN_MAX_ATTEMPTS,
                        delay_ms,
                        error = %e,
                        "database open failed, retrying"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                last_err = Some(e);
            }
        }
    }
    Err(Box::new(ServerError::Core(CoreError::Store(
        last_err.expect("at least one attempt was made"),
    ))))
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
