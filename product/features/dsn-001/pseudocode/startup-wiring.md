# Pseudocode: startup-wiring

**Files**:
- `crates/unimatrix-server/src/main.rs` (modified — `tokio_main_daemon` and `tokio_main_stdio` only)
- `crates/unimatrix-server/src/background.rs` (modified — `spawn_background_tick` and `background_tick_loop` gain `Arc<ConfidenceParams>`)

## Purpose

Wires the config loader into the two async server entry points (`tokio_main_daemon`,
`tokio_main_stdio`). Config is loaded once immediately after `ensure_data_directory()`
returns `paths`, and the resolved values are distributed to subsystem constructors as
plain parameters. No `Arc<UnimatrixConfig>` is stored on any struct (ADR-002).

`tokio_main_bridge`, `Command::Hook`, `Command::Export`, `Command::Import`,
`Command::Version`, `Command::ModelDownload`, and `run_stop` are NOT modified —
they never call `load_config` (R-20 constraint).

The `background.rs` changes add `Arc<ConfidenceParams>` to `spawn_background_tick`
and thread it into `background_tick_loop` so all confidence computations inside
the tick use operator-configured weights.

---

## New `use` Imports (main.rs)

```
// Add to existing use block at the top of main.rs:
use std::collections::HashSet;

use unimatrix_store::Capability;
use unimatrix_server::infra::config::{load_config, resolve_confidence_params};
use unimatrix_engine::confidence::ConfidenceParams;
```

The `dirs` crate is needed for `home_dir()`:
```
use dirs;   // added to unimatrix-server/Cargo.toml if not already present
```

---

## Config Load Block (inserted in both `tokio_main_daemon` and `tokio_main_stdio`)

Insertion point: immediately after `ensure_data_directory()` succeeds and before
`open_store_with_retry`. Place after the tracing log of project paths.

```
// ── dsn-001: Load external config ─────────────────────────────────────────────
// dirs::home_dir() returns None in rootless/container environments.
// When None: log a warning and proceed with compiled defaults (R-15).
let config = match dirs::home_dir() {
    Some(home) => {
        match load_config(&home, &paths.data_dir) {
            Ok(cfg) => {
                tracing::info!(
                    preset = ?cfg.profile.preset,
                    "config loaded"
                );
                cfg
            }
            Err(e) => {
                // Config errors are not fatal: log and fall back to defaults.
                // This preserves zero-config behavior (AC-01).
                tracing::warn!(error = %e, "config load failed; using compiled defaults");
                unimatrix_server::infra::config::UnimatrixConfig::default()
            }
        }
    }
    None => {
        tracing::warn!("home directory not found; using compiled defaults (R-15)");
        unimatrix_server::infra::config::UnimatrixConfig::default()
    }
};

// Resolve ConfidenceParams from preset/weights. Infallible when config is valid;
// the only error path is custom preset with missing freshness_half_life_hours,
// which validate_config already caught. If resolve fails (e.g. load fell back
// to defaults above), this produces ConfidenceParams::default() via the
// Collaborative preset path.
let confidence_params = Arc::new(
    resolve_confidence_params(&config)
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "confidence params resolution failed; using defaults");
            ConfidenceParams::default()
        })
);

// Extract concrete values for subsystem constructors.
// None of these are stored as Arc<UnimatrixConfig> on any struct (ADR-002).
let knowledge_categories: Vec<String> = config.knowledge.categories.clone();
let boosted_categories: HashSet<String> =
    config.knowledge.boosted_categories.iter().cloned().collect();
let server_instructions: Option<String> = config.server.instructions.clone();
let permissive: bool = config.agents.default_trust == "permissive";
let session_caps: Vec<Capability> = config.agents.session_capabilities.iter()
    .filter_map(|s| match s.as_str() {
        "Read"   => Some(Capability::Read),
        "Write"  => Some(Capability::Write),
        "Search" => Some(Capability::Search),
        _        => None,  // unreachable: validate_config guards this
    })
    .collect();
// ── end dsn-001 config load ────────────────────────────────────────────────────
```

---

## CategoryAllowlist Construction (changed)

```
// BEFORE:
let categories = Arc::new(CategoryAllowlist::new());

// AFTER:
let categories = Arc::new(
    CategoryAllowlist::from_categories(knowledge_categories)
);
```

---

## AgentRegistry Construction (changed)

```
// BEFORE:
let registry = Arc::new(AgentRegistry::new(Arc::clone(&store))?);

// AFTER:
let registry = Arc::new(
    AgentRegistry::new(Arc::clone(&store), permissive, session_caps)?
);
```

The `registry.bootstrap_defaults()` call immediately after is unchanged.

---

## UnimatrixServer Construction (changed)

```
// BEFORE:
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
);

// AFTER:
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
    server_instructions,   // NEW: Option<String> from config
);
```

---

## ServiceLayer / SearchService (changed)

`ServiceLayer::new` must be extended to accept `boosted_categories: HashSet<String>`
and thread it to `SearchService::new`. The delivery agent must update
`ServiceLayer::new` and `ServiceLayer::with_rate_config` to add this parameter.

```
// BEFORE:
let services = unimatrix_server::services::ServiceLayer::new(
    Arc::clone(&store),
    Arc::clone(&vector_index),
    Arc::clone(&async_vector_store),
    Arc::clone(&store),
    Arc::clone(&embed_handle),
    Arc::clone(&adapt_service),
    Arc::clone(&audit),
    Arc::clone(&usage_dedup),
);

// AFTER:
let services = unimatrix_server::services::ServiceLayer::new(
    Arc::clone(&store),
    Arc::clone(&vector_index),
    Arc::clone(&async_vector_store),
    Arc::clone(&store),
    Arc::clone(&embed_handle),
    Arc::clone(&adapt_service),
    Arc::clone(&audit),
    Arc::clone(&usage_dedup),
    boosted_categories,     // NEW: HashSet<String> from config
);
```

The same change applies to both the daemon and stdio call sites.

Note: `uds_listener::start_uds_listener` also constructs a `ServiceLayer` internally
(in `listener.rs`). That internal construction has no access to the operator config
at call time — it will use `SearchService` with `HashSet::from(["lesson-learned"])` as
the default. This is acceptable: the UDS listener's search path uses the same default
behavior as pre-dsn-001. Operators who need custom boosted categories for hook-path
search will require a follow-up (not in dsn-001 scope).

---

## `spawn_background_tick` (changed in background.rs)

### New Parameter

```
// BEFORE:
pub fn spawn_background_tick(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    session_registry: Arc<SessionRegistry>,
    entry_store: Arc<Store>,
    pending_entries: Arc<Mutex<PendingEntriesAnalysis>>,
    tick_metadata: Arc<Mutex<TickMetadata>>,
    training_service: Option<Arc<TrainingService>>,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,
    supersession_state: SupersessionStateHandle,
    contradiction_cache: ContradictionScanCacheHandle,
    audit_log: Arc<AuditLog>,
    auto_quarantine_cycles: u32,
) -> tokio::task::JoinHandle<()>

// AFTER: add confidence_params parameter
pub fn spawn_background_tick(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    session_registry: Arc<SessionRegistry>,
    entry_store: Arc<Store>,
    pending_entries: Arc<Mutex<PendingEntriesAnalysis>>,
    tick_metadata: Arc<Mutex<TickMetadata>>,
    training_service: Option<Arc<TrainingService>>,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,
    supersession_state: SupersessionStateHandle,
    contradiction_cache: ContradictionScanCacheHandle,
    audit_log: Arc<AuditLog>,
    auto_quarantine_cycles: u32,
    confidence_params: Arc<ConfidenceParams>,   // NEW: from resolve_confidence_params
) -> tokio::task::JoinHandle<()>
```

### Inner Spawn (supervisor body)

The inner `tokio::spawn(background_tick_loop(...))` call inside the supervisor loop
gains the same parameter:

```
// Inside the loop body of the supervisor:
let inner_handle = tokio::spawn(background_tick_loop(
    Arc::clone(&store),
    Arc::clone(&vector_index),
    Arc::clone(&embed_service),
    Arc::clone(&adapt_service),
    Arc::clone(&session_registry),
    Arc::clone(&entry_store),
    Arc::clone(&pending_entries),
    Arc::clone(&tick_metadata),
    training_service.clone(),
    confidence_state.clone(),
    effectiveness_state.clone(),
    supersession_state.clone(),
    Arc::clone(&contradiction_cache),
    Arc::clone(&audit_log),
    auto_quarantine_cycles,
    Arc::clone(&confidence_params),    // NEW
));
```

### `background_tick_loop` Signature (changed)

`background_tick_loop` is the private `async fn` that receives all parameters and
runs a single tick cycle. It gains the same new parameter:

```
// AFTER: add confidence_params parameter at end of existing list
async fn background_tick_loop(
    // ... all existing parameters unchanged ...
    auto_quarantine_cycles: u32,
    confidence_params: Arc<ConfidenceParams>,   // NEW
)
```

Inside `background_tick_loop`, all calls to `compute_confidence(entry, now)` become
`compute_confidence(entry, now, &confidence_params)`. The delivery agent must locate
all such calls within `background.rs` and update them.

---

## `spawn_background_tick` Call Sites in main.rs (changed)

Both `tokio_main_daemon` and `tokio_main_stdio` call `spawn_background_tick`. Both
call sites gain the `confidence_params` argument as the last parameter:

```
// BEFORE (daemon):
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
    supersession_state_handle,
    contradiction_cache_handle,
    Arc::clone(&audit),
    auto_quarantine_cycles,
);

// AFTER (daemon):
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
    supersession_state_handle,
    contradiction_cache_handle,
    Arc::clone(&audit),
    auto_quarantine_cycles,
    Arc::clone(&confidence_params),    // NEW
);
```

The same change applies to the stdio call site.

---

## Sequencing Constraint

The config load block must appear in this exact order within each async entry point:

```
1. ensure_data_directory()        → paths
2. [tracing log of paths]
3. dirs::home_dir() → load_config() or default  ← NEW
4. resolve_confidence_params()                  ← NEW
5. extract: knowledge_categories, boosted_categories,
            server_instructions, permissive, session_caps  ← NEW
6. handle_stale_pid_file()
7. open_store_with_retry()
8. PidGuard::acquire()
9. handle_stale_socket()
10. VectorIndex init
11. EmbedServiceHandle::new()
12. AgentRegistry::new(store, permissive, session_caps)     ← CHANGED
13. AuditLog::new()
14. AsyncVectorStore + VectorAdapter
15. CategoryAllowlist::from_categories(knowledge_categories) ← CHANGED
16. AdaptationService::new()
17. SessionRegistry::new()
18. PendingEntriesAnalysis::new()
19. ServiceLayer::new(..., boosted_categories)               ← CHANGED
20. start_uds_listener()
21. UnimatrixServer::new(..., server_instructions)           ← CHANGED
22. extract state handles (confidence, effectiveness, supersession, contradiction)
23. parse_auto_quarantine_cycles()
24. spawn_background_tick(..., confidence_params)            ← CHANGED
25. [daemon: daemon_token + MCP acceptor + signal handler]
    [stdio: LifecycleHandles + serve over stdio]
```

Config values are available at step 5, before any subsystem constructor. This satisfies
the constraint that no subsystem runs with hardcoded values when a config file is present.

---

## `dirs` Crate Dependency

`dirs` must be added to `unimatrix-server/Cargo.toml`:

```toml
dirs = "5"
```

Check `cargo tree` after adding to confirm no version conflicts. If `dirs` is already
a transitive dependency of another crate in the workspace, match that version.

---

## Key Test Scenarios

1. **No config file — compiled defaults used** (AC-01):
   - No `~/.unimatrix/config.toml` present.
   - Server starts normally; `CategoryAllowlist` has the 8 `INITIAL_CATEGORIES`.
   - `ConfidenceParams` equals `ConfidenceParams::default()`.
   - All existing integration tests pass.

2. **`dirs::home_dir()` returns `None` — warns and uses defaults** (R-15):
   - Simulate by mocking or by code review of the `None` arm.
   - `tracing::warn!` is emitted; server starts with `UnimatrixConfig::default()`.
   - No panic, no abort.

3. **Config load error — warns and uses defaults** (R-15):
   - Introduce a malformed `~/.unimatrix/config.toml`.
   - `load_config` returns `Err`; server logs warn and uses defaults.
   - Server starts successfully.

4. **Custom config categories — allowlist populated from config** (AC-02):
   - `~/.unimatrix/config.toml` has `[knowledge] categories = ["ruling", "statute"]`.
   - `CategoryAllowlist::from_categories(["ruling", "statute"])` is called.
   - `context_store` with `category = "ruling"` succeeds.
   - `context_store` with `category = "outcome"` fails (not in list).

5. **Empirical preset — background tick uses empirical params** (IR-04):
   - Config: `[profile] preset = "empirical"`.
   - `confidence_params.w_fresh == 0.34`, `confidence_params.freshness_half_life_hours == 24.0`.
   - Background tick receives `Arc<ConfidenceParams>` with these values.

6. **Hook and bridge paths do not call `load_config`** (R-20):
   - Grep gate: `grep -n "load_config" main.rs` returns matches only inside
     `tokio_main_daemon` and `tokio_main_stdio`.
   - Zero matches in hook dispatch, bridge path, export/import, version, stop.

---

## Error Handling

`load_config` failures are non-fatal: the `match` arm logs a `tracing::warn!` and
falls back to `UnimatrixConfig::default()`. The server continues startup normally.

`resolve_confidence_params` failures are likewise handled with a fallback to
`ConfidenceParams::default()` via `unwrap_or_else`.

The only fatal startup error path from config is `validate_config` returning
`ConfigError::CustomPresetMissingHalfLife` — but this is surfaced inside
`load_config` and caught by the non-fatal handler above (fallback to defaults).
If operators want hard failure on bad config, a future flag could make this fatal;
for dsn-001, graceful degradation is the specified behavior.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` `main.rs` — no patterns found
  for config injection at startup. This is the first config externalization in the
  project.
- Deviations from established patterns: none. The `dirs::home_dir() → None` fallback
  pattern follows SPECIFICATION.md §R-15 exactly.
