//! Unimatrix MCP knowledge server entry point.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
use unimatrix_core::{CoreError, EmbedConfig, StoreAdapter, Store, VectorAdapter, VectorConfig, VectorIndex};
use unimatrix_store::StoreError;

use unimatrix_adapt::{AdaptConfig, AdaptationService};
use unimatrix_server::audit::AuditLog;
use unimatrix_server::categories::CategoryAllowlist;
use unimatrix_server::embed_handle::EmbedServiceHandle;
use unimatrix_server::error::ServerError;
use unimatrix_server::pidfile;
use unimatrix_server::project;
use unimatrix_server::registry::AgentRegistry;
use unimatrix_server::server::{PendingEntriesAnalysis, UnimatrixServer};
use unimatrix_server::shutdown::{self, LifecycleHandles};
use unimatrix_server::uds_listener;

/// Maximum number of attempts to open the database when the lock is held.
const DB_OPEN_MAX_ATTEMPTS: u32 = 3;

/// Delay between database open retry attempts.
const DB_OPEN_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Timeout for waiting on a stale process to exit after SIGTERM.
const STALE_PROCESS_TIMEOUT: Duration = Duration::from_secs(5);

/// Unimatrix MCP knowledge server.
#[derive(Parser)]
#[command(name = "unimatrix-server", about = "Unimatrix MCP knowledge server")]
struct Cli {
    /// Override project root directory.
    #[arg(long)]
    project_dir: Option<PathBuf>,

    /// Enable verbose logging.
    #[arg(long, short)]
    verbose: bool,

    /// Subcommand (hook, or none for server mode).
    #[command(subcommand)]
    command: Option<Command>,
}

/// Subcommands for the unimatrix-server binary.
#[derive(Subcommand)]
enum Command {
    /// Handle a Claude Code lifecycle hook event.
    ///
    /// Reads JSON from stdin, connects to the running server via UDS,
    /// and dispatches the event. No tokio runtime is initialized.
    Hook {
        /// The hook event name (e.g., SessionStart, Stop, Ping).
        event: String,
    },
}

/// Entry point: branches between hook subcommand (sync) and server (async).
///
/// The hook path runs pure synchronous code with no tokio runtime (ADR-002).
/// The server path initializes tokio for the full MCP server.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Hook { event }) => {
            // Sync path: NO tokio, NO tracing init, NO database open
            // Minimal startup for <50ms budget
            unimatrix_server::hook::run(event, cli.project_dir)
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
    let paths = project::ensure_data_directory(cli.project_dir.as_deref())
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
    let session_registry = Arc::new(unimatrix_server::session::SessionRegistry::new());

    // Create pending entries analysis accumulator shared between UDS listener and MCP server (col-009)
    let pending_entries_analysis = Arc::new(Mutex::new(PendingEntriesAnalysis::new()));

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
    )
    .await?;

    // Build server
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
    server.pending_entries_analysis = pending_entries_analysis;
    server.session_registry = Arc::clone(&session_registry);

    // Prepare lifecycle handles for shutdown
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
    };

    // Serve over stdio
    tracing::info!("serving MCP over stdio");
    let running = server
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| ServerError::Shutdown(e.to_string()))?;

    // Wait for session close or signal, then shutdown.
    // Flock-based PidGuard handles zombie cleanup at next startup.
    let waiting = async { let _ = running.waiting().await; };
    shutdown::graceful_shutdown(lifecycle_handles, waiting).await?;

    tracing::info!("unimatrix server exited cleanly");
    Ok(())
}

/// Attempt to open the database, retrying on `DatabaseAlreadyOpen`.
///
/// Makes up to [`DB_OPEN_MAX_ATTEMPTS`] attempts with [`DB_OPEN_RETRY_DELAY`]
/// between each. This handles the race where a stale process received SIGTERM
/// but has not yet released the database lock.
fn open_store_with_retry(
    db_path: &std::path::Path,
) -> Result<Arc<Store>, Box<dyn std::error::Error>> {
    for attempt in 1..=DB_OPEN_MAX_ATTEMPTS {
        match Store::open(db_path) {
            Ok(s) => return Ok(Arc::new(s)),
            Err(StoreError::Database(redb::DatabaseError::DatabaseAlreadyOpen)) => {
                if attempt < DB_OPEN_MAX_ATTEMPTS {
                    tracing::warn!(
                        attempt,
                        max_attempts = DB_OPEN_MAX_ATTEMPTS,
                        "database locked by another process, retrying in {}s",
                        DB_OPEN_RETRY_DELAY.as_secs()
                    );
                    std::thread::sleep(DB_OPEN_RETRY_DELAY);
                } else {
                    return Err(ServerError::DatabaseLocked(db_path.to_path_buf()).into());
                }
            }
            Err(e) => return Err(ServerError::Core(CoreError::Store(e)).into()),
        }
    }

    // Unreachable: the loop either returns Ok, exits the process, or returns Err.
    unreachable!()
}
