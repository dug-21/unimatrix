//! Unimatrix MCP knowledge server entry point.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
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
use unimatrix_server::server::UnimatrixServer;
use unimatrix_server::shutdown::{self, LifecycleHandles};

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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

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

    // Write PID file now that we hold the database lock
    if let Err(e) = pidfile::write_pid_file(&paths.pid_path) {
        tracing::warn!(error = %e, "failed to write PID file; continuing without it");
    }

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

    // Build server
    let server = UnimatrixServer::new(
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

    // Prepare lifecycle handles for shutdown
    let lifecycle_handles = LifecycleHandles {
        store,
        vector_index,
        vector_dir: paths.vector_dir.clone(),
        registry,
        audit,
        pid_path: paths.pid_path.clone(),
        adapt_service,
        data_dir: paths.data_dir.clone(),
    };

    // Serve over stdio
    tracing::info!("serving MCP over stdio");
    let running = server
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| ServerError::Shutdown(e.to_string()))?;

    // Wait for session close or signal, then shutdown
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
                    eprintln!("error: database is locked by another process");
                    eprintln!("  path: {}", db_path.display());
                    eprintln!(
                        "  hint: kill the other unimatrix-server process, or run: lsof {}",
                        db_path.display()
                    );
                    std::process::exit(1);
                }
            }
            Err(e) => return Err(ServerError::Core(CoreError::Store(e)).into()),
        }
    }

    // Unreachable: the loop either returns Ok, exits the process, or returns Err.
    unreachable!()
}
