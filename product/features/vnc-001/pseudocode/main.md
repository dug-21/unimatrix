# Pseudocode: main.rs + lib.rs (C1 — Binary Entry Point)

## Purpose

The `#[tokio::main]` entry point. Wires all subsystems together, serves MCP over stdio, handles shutdown. `lib.rs` declares modules and pub exports for integration testing.

## lib.rs — Module Declarations

```
#![forbid(unsafe_code)]

pub mod audit;
pub mod embed_handle;
pub mod error;
pub mod identity;
pub mod project;
pub mod registry;
pub mod server;
pub mod shutdown;
pub mod tools;
```

All modules are pub for integration testing via `use unimatrix_server::*`.

## main.rs — Entry Point

### CLI Args (clap)

```
#[derive(Parser)]
#[command(name = "unimatrix-server", about = "Unimatrix MCP knowledge server")]
struct Cli {
    /// Override project root directory
    #[arg(long)]
    project_dir: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(long, short)]
    verbose: bool,
}
```

### main() Function

```
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Parse CLI args
    let cli = Cli::parse();

    // Step 2: Initialize tracing
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)  // MCP uses stdout for protocol; logs go to stderr
        .init();

    tracing::info!("starting unimatrix server");

    // Step 3: Initialize project data directory
    let paths = project::ensure_data_directory(
        cli.project_dir.as_deref()
    ).map_err(|e| ServerError::ProjectInit(e.to_string()))?;

    tracing::info!(
        project_root = %paths.project_root.display(),
        project_hash = %paths.project_hash,
        data_dir = %paths.data_dir.display(),
        "project initialized"
    );

    // Step 4: Open database
    let store = Arc::new(Store::open(&paths.db_path)
        .map_err(|e| ServerError::Core(CoreError::Store(e)))?);

    // Step 5: Initialize vector index
    let vector_config = VectorConfig::default();  // 384 dims, 16 connections, ef=200
    let meta_path = paths.vector_dir.join("unimatrix-vector.meta");

    let vector_index = if meta_path.exists() {
        tracing::info!("loading existing vector index");
        Arc::new(VectorIndex::load(
            Arc::clone(&store),
            vector_config,
            &paths.vector_dir,
        ).map_err(|e| ServerError::Core(CoreError::Vector(e)))?)
    } else {
        tracing::info!("creating new vector index");
        Arc::new(VectorIndex::new(
            Arc::clone(&store),
            vector_config,
        ).map_err(|e| ServerError::Core(CoreError::Vector(e)))?)
    };

    // Step 6: Initialize embedding service (lazy — background task)
    let embed_handle = EmbedServiceHandle::new();
    embed_handle.start_loading(EmbedConfig::default());

    // Step 7: Initialize agent registry and bootstrap defaults
    let registry = Arc::new(AgentRegistry::new(Arc::clone(&store))?);
    registry.bootstrap_defaults()?;

    // Step 8: Initialize audit log
    let audit = Arc::new(AuditLog::new(Arc::clone(&store)));

    // Step 9: Build adapters and async wrappers
    let store_adapter = StoreAdapter::new(Arc::clone(&store));
    let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));

    let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
    let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

    // Step 10: Build server
    let server = UnimatrixServer::new(
        async_entry_store,
        async_vector_store,
        Arc::clone(&embed_handle),
        Arc::clone(&registry),
        Arc::clone(&audit),
    );

    // Step 11: Prepare lifecycle handles (for shutdown)
    let lifecycle_handles = LifecycleHandles {
        store,          // the original Arc<Store>, not a clone
        vector_index,   // the original Arc<VectorIndex>
        vector_dir: paths.vector_dir.clone(),
    };

    // Step 12: Serve over stdio
    tracing::info!("serving MCP over stdio");
    let running = server
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| ServerError::Shutdown(e.to_string()))?;

    // Step 13: Wait for session close or signal, then shutdown
    shutdown::graceful_shutdown(lifecycle_handles, running).await?;

    tracing::info!("unimatrix server exited cleanly");
    Ok(())
}
```

## Key Design Points

1. **Tracing to stderr**: MCP protocol uses stdout. All logs go to stderr via `tracing_subscriber`.
2. **Arc reference management**: `store` and `vector_index` are created as `Arc<T>`. Clones go into adapters and the server. The original Arcs go into `LifecycleHandles` for shutdown.
3. **Embed lazy init**: `EmbedServiceHandle::new()` is instant. `start_loading()` spawns the background task. Server proceeds to MCP init without waiting.
4. **Error propagation**: Startup errors are `Box<dyn Error>` (exit with error message). Runtime errors are handled by rmcp's tool dispatch.
5. **store clone count**: The store Arc is cloned to: (1) VectorIndex, (2) AgentRegistry, (3) AuditLog, (4) StoreAdapter, (5) LifecycleHandles. During shutdown, dropping the server drops (4)'s chain. Dropping registry, audit drops (2) and (3). VectorIndex is dropped separately. LifecycleHandles' (5) is the last reference for try_unwrap.

Wait — the VectorIndex already holds an `Arc<Store>` internally (from `VectorIndex::new(store, config)`). So the clone count is actually: VectorIndex internal, AgentRegistry, AuditLog, StoreAdapter -> AsyncEntryStore, LifecycleHandles. For try_unwrap to succeed, ALL of these must be dropped before we try to unwrap the LifecycleHandles' store.

The shutdown function needs to ensure this ordering. Since `graceful_shutdown` consumes the RunningService (which holds the server clone), and the server clone holds the async wrappers (which hold Arc<StoreAdapter> which holds Arc<Store>), dropping the running service should drop the server's store references.

But AgentRegistry and AuditLog also hold `Arc<Store>`. These are created in main and live until main exits. The `graceful_shutdown` function returns, then main's scope ends, dropping registry and audit. But by that point, main has already moved `store` into `LifecycleHandles` which was moved into `graceful_shutdown`.

**Important**: The `graceful_shutdown` function must receive and explicitly drop the registry and audit Arcs (or they must be dropped by main before the try_unwrap). Let me revise the design.

Revised: `LifecycleHandles` also holds the registry and audit references so they can be dropped before try_unwrap:

```
struct LifecycleHandles {
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    vector_dir: PathBuf,
    // Additional references that hold Arc<Store> clones:
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
}
```

In `graceful_shutdown`, after dumping vectors:
```
drop(handles.registry);    // drops its Arc<Store> clone
drop(handles.audit);       // drops its Arc<Store> clone
drop(handles.vector_index); // drops VectorIndex which holds Arc<Store>
// Now try_unwrap(handles.store)
```

This ensures all Arc<Store> clones are dropped before try_unwrap.

## Error Handling

- Startup errors (project init, store open, vector load): terminate with error message
- Runtime errors: handled per-request by rmcp tool dispatch
- Shutdown errors: logged as warnings, exit 0 regardless

## Key Test Scenarios

1. Binary compiles and runs (AC-01)
2. Server completes MCP initialize handshake (AC-02)
3. Server responds to ping
4. `--project-dir` override works
5. `--verbose` enables debug logging
6. All subsystems initialized in correct order
7. Graceful shutdown on session close
8. Graceful shutdown on SIGTERM
