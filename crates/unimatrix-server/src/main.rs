//! Unimatrix knowledge engine entry point.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use unimatrix_adapt::{AdaptConfig, AdaptationService};
use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
use unimatrix_core::{
    CoreError, EmbedConfig, Store, StoreAdapter, VectorAdapter, VectorConfig, VectorIndex,
};
use unimatrix_server::error::ServerError;
use unimatrix_server::infra::audit::AuditLog;
use unimatrix_server::infra::categories::CategoryAllowlist;
use unimatrix_server::infra::embed_handle::EmbedServiceHandle;
use unimatrix_server::infra::pidfile;
use unimatrix_server::infra::registry::AgentRegistry;
use unimatrix_server::infra::shutdown::{self, LifecycleHandles};
use unimatrix_server::infra::usage_dedup::UsageDedup;
use unimatrix_server::project;
use unimatrix_server::server::{PendingEntriesAnalysis, UnimatrixServer};
use unimatrix_server::uds_listener;

/// Timeout for waiting on a stale process to exit after SIGTERM.
/// Increased from 5s to 10s to accommodate heavier shutdown since vnc-006 (#92).
const STALE_PROCESS_TIMEOUT: Duration = Duration::from_secs(10);

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

    /// Subcommand (hook, version, model-download, or none for server mode).
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
    /// Used by npm postinstall to pre-download the embedding model.
    /// Synchronous path, no tokio runtime.
    ModelDownload,
}

/// Entry point: branches between hook subcommand (sync) and server (async).
///
/// The hook path runs pure synchronous code with no tokio runtime (ADR-002).
/// The server path initializes tokio for the full MCP server.
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

    match cli.command {
        Some(Command::Hook { event }) => {
            // Sync path: NO tokio, NO tracing init, NO database open
            // Minimal startup for <50ms budget
            unimatrix_server::uds::hook::run(event, cli.project_dir)
        }
        Some(Command::Export { output }) => {
            // Sync path: NO tokio, like Hook
            unimatrix_server::export::run_export(cli.project_dir.as_deref(), output.as_deref())
        }
        Some(Command::Import {
            input,
            skip_hash_validation,
            force,
        }) => {
            // Sync path: NO tokio, like Hook and Export
            unimatrix_server::import::run_import(
                cli.project_dir.as_deref(),
                &input,
                skip_hash_validation,
                force,
            )
        }
        Some(Command::Version) => {
            // Sync path: NO tokio
            handle_version(cli.project_dir)
        }
        Some(Command::ModelDownload) => {
            // Sync path: NO tokio
            handle_model_download()
        }
        None => {
            // Async path: full server with tokio runtime
            tokio_main(cli)
        }
    }
}

/// Tokio-based server entry point (called from main when no subcommand).
#[tokio::main]
async fn tokio_main(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing (logs to stderr — stdout is for MCP protocol)
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("starting unimatrix server");

    // Initialize project data directory
    let paths = project::ensure_data_directory(cli.project_dir.as_deref(), None)
        .map_err(|e| ServerError::ProjectInit(e.to_string()))?;

    tracing::info!(
        project_root = %paths.project_root.display(),
        project_hash = %paths.project_hash,
        data_dir = %paths.data_dir.display(),
        "project initialized"
    );

    // Handle stale PID file before attempting to open the database
    match pidfile::handle_stale_pid_file(&paths.pid_path, STALE_PROCESS_TIMEOUT) {
        Ok(true) => {} // Resolved or no stale process.
        Ok(false) => {
            tracing::warn!("stale process did not exit; will attempt database open anyway");
        }
        Err(e) => {
            tracing::warn!(error = %e, "PID file handling failed; continuing startup");
        }
    }

    // Open database with retry loop for lock contention
    let store = open_store_with_retry(&paths.db_path)?;

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

    // Handle stale socket file (unconditional unlink per ADR-004)
    uds_listener::handle_stale_socket(&paths.socket_path)?;

    // Initialize vector index
    let vector_config = VectorConfig::default();
    let meta_path = paths.vector_dir.join("unimatrix-vector.meta");

    let vector_index = if meta_path.exists() {
        tracing::info!("loading existing vector index");
        Arc::new(
            VectorIndex::load(Arc::clone(&store), vector_config, &paths.vector_dir)
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?,
        )
    } else {
        tracing::info!("creating new vector index");
        Arc::new(
            VectorIndex::new(Arc::clone(&store), vector_config)
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?,
        )
    };

    // Initialize embedding service (lazy — background task)
    let embed_handle = EmbedServiceHandle::new();
    embed_handle.start_loading(EmbedConfig::default());

    // Initialize agent registry and bootstrap defaults
    let registry = Arc::new(AgentRegistry::new(Arc::clone(&store))?);
    registry.bootstrap_defaults()?;

    // Initialize audit log
    let audit = Arc::new(AuditLog::new(Arc::clone(&store)));

    // Build adapters and async wrappers
    let store_adapter = StoreAdapter::new(Arc::clone(&store));
    let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));

    let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
    let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

    // Initialize category allowlist
    let categories = Arc::new(CategoryAllowlist::new());

    // Initialize adaptation service (crt-006)
    let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));
    if let Err(e) = adapt_service.load_state(&paths.data_dir) {
        tracing::warn!("adaptation state load failed: {e}, starting fresh");
    } else {
        let training_gen = adapt_service.training_generation();
        if training_gen > 0 {
            tracing::info!(generation = training_gen, "adaptation state restored");
        }
    }

    // Create session registry for hook IPC (col-008)
    let session_registry = Arc::new(unimatrix_server::infra::session::SessionRegistry::new());

    // Create pending entries analysis accumulator shared between UDS listener and MCP server (col-009)
    let pending_entries_analysis = Arc::new(Mutex::new(PendingEntriesAnalysis::new()));

    // Build shared ServiceLayer for UDS and MCP transports (vnc-006, vnc-009)
    let usage_dedup = Arc::new(UsageDedup::new());
    let services = unimatrix_server::services::ServiceLayer::new(
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&async_vector_store),
        Arc::clone(&async_entry_store),
        Arc::clone(&embed_handle),
        Arc::clone(&adapt_service),
        Arc::clone(&audit),
        Arc::clone(&usage_dedup),
    );

    // Start UDS listener for hook IPC (expanded signature per col-007 ADR-001, col-008, col-009)
    let server_uid = nix::unistd::getuid().as_raw();
    let (uds_handle, socket_guard) = uds_listener::start_uds_listener(
        &paths.socket_path,
        Arc::clone(&store),
        Arc::clone(&embed_handle),
        Arc::clone(&async_vector_store),
        Arc::clone(&async_entry_store),
        Arc::clone(&adapt_service),
        Arc::clone(&session_registry),
        Arc::clone(&pending_entries_analysis),
        server_uid,
        env!("CARGO_PKG_VERSION").to_string(),
        services.clone(),
        Arc::clone(&audit),
    )
    .await?;

    // Build server
    let async_entry_store_for_tick = Arc::clone(&async_entry_store);
    let mut server = UnimatrixServer::new(
        async_entry_store,
        async_vector_store,
        Arc::clone(&embed_handle),
        Arc::clone(&registry),
        Arc::clone(&audit),
        categories,
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&adapt_service),
    );
    // Share pending_entries_analysis and session_registry with the MCP server (col-009)
    server.pending_entries_analysis = Arc::clone(&pending_entries_analysis);
    server.session_registry = Arc::clone(&session_registry);

    // crt-019: extract ConfidenceStateHandle before services is moved.
    let confidence_state_handle = services.confidence_state_handle();
    // crt-018b: extract EffectivenessStateHandle before services is moved.
    let effectiveness_state_handle = services.effectiveness_state_handle();
    // GH #264: extract SupersessionStateHandle before services is moved.
    let supersession_state_handle = services.supersession_state_handle();
    // GH #278: extract ContradictionScanCacheHandle before services is moved.
    let contradiction_cache_handle = services.contradiction_cache_handle();

    // crt-018b: parse auto-quarantine threshold at startup (Constraint 14).
    // Value 0 disables auto-quarantine; values > 1000 cause a startup error.
    let auto_quarantine_cycles = unimatrix_server::background::parse_auto_quarantine_cycles()
        .map_err(ServerError::ProjectInit)?;

    // Spawn background tick for automated maintenance + extraction (col-013)
    let tick_handle = unimatrix_server::background::spawn_background_tick(
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&embed_handle),
        Arc::clone(&adapt_service),
        Arc::clone(&session_registry),
        async_entry_store_for_tick,
        Arc::clone(&pending_entries_analysis),
        Arc::clone(&server.tick_metadata),
        None, // TrainingService: wired in future integration step
        confidence_state_handle,
        effectiveness_state_handle, // crt-018b: shared with search/briefing paths
        supersession_state_handle,  // GH #264: shared with SearchService
        contradiction_cache_handle, // GH #278: shared with StatusService
        Arc::clone(&audit),         // crt-018b: for tick_skipped audit events
        auto_quarantine_cycles,     // crt-018b: auto-quarantine threshold
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
        socket_guard: Some(socket_guard),
        uds_handle: Some(uds_handle),
        tick_handle: Some(tick_handle),
        services: Some(services),
    };

    // Serve over stdio
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
    match running.waiting().await {
        Ok(reason) => tracing::info!(?reason, "MCP transport closed"),
        Err(e) => tracing::error!(error = %e, "MCP transport task failed"),
    }

    // Run lifecycle shutdown (vector dump, adapt save, DB compaction).
    shutdown::graceful_shutdown(lifecycle_handles).await?;

    tracing::info!("unimatrix server exited cleanly");
    Ok(())
}

/// Print version string to stdout and exit.
///
/// When `--project-dir` is provided, also pre-creates the data directory and
/// database (used by `npx unimatrix init` to ensure DB exists before first run).
fn handle_version(project_dir: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(dir) = project_dir {
        let paths = project::ensure_data_directory(Some(&dir), None)
            .map_err(|e| ServerError::ProjectInit(e.to_string()))?;
        let _store = Store::open(&paths.db_path)?;
        eprintln!("database initialized at {}", paths.db_path.display());
    }

    println!("unimatrix {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

/// Download the ONNX embedding model to cache.
///
/// Uses `EmbedConfig::default()` to resolve the cache directory, then calls
/// `ensure_model()` synchronously. Progress messages go to stderr (stdout
/// is reserved for structured output / MCP protocol).
fn handle_model_download() -> Result<(), Box<dyn std::error::Error>> {
    let config = EmbedConfig::default();
    let cache_dir = config.resolve_cache_dir();

    eprintln!("Downloading ONNX model to {}...", cache_dir.display());

    match unimatrix_embed::ensure_model(config.model, &cache_dir) {
        Ok(model_dir) => {
            eprintln!("Model ready: {}", model_dir.display());
            Ok(())
        }
        Err(e) => {
            eprintln!("Model download failed: {e}");
            Err(Box::new(e))
        }
    }
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
fn open_store_with_retry(
    db_path: &std::path::Path,
) -> Result<Arc<Store>, Box<dyn std::error::Error>> {
    let mut last_err = None;
    for attempt in 1..=DB_OPEN_MAX_ATTEMPTS {
        match Store::open(db_path) {
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
                    std::thread::sleep(Duration::from_millis(delay_ms));
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
