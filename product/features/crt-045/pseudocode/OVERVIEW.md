# crt-045 Pseudocode Overview
# Eval Harness — Wire TypedGraphState Rebuild into EvalServiceLayer

## Components Involved

| Component | File | Role in crt-045 |
|-----------|------|----------------|
| `EvalServiceLayer` | `eval/profile/layer.rs` | Primary change site: rebuild call + write-back + accessor |
| `ppr-expander-enabled.toml` | `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` | Fix TOML parse failure (ADR-005) |
| `layer_tests.rs` | `eval/profile/layer_tests.rs` | New integration tests (AC-06, AC-05) |

Components NOT changed (read-only reference):
- `services/typed_graph.rs` — `TypedGraphState::rebuild()` and `TypedGraphStateHandle` (already correct)
- `services/mod.rs` — `ServiceLayer::with_rate_config()` and `typed_graph_handle()` (unchanged)
- `services/search.rs` — `if !use_fallback` guard (behavioral target; only fixed by populating the handle)

---

## Data Flow Between Components

```
EvalServiceLayer::from_profile() [async constructor]
  │
  │  [Step 5 — existing]
  ├─ SqlxStore::open_readonly(db_path) -> store_arc: Arc<Store>
  │
  │  [Step 5b — NEW]
  ├─ TypedGraphState::rebuild(&*store_arc).await
  │     ok  -> rebuilt_state: Some(TypedGraphState { use_fallback: false, typed_graph: *, all_entries: * })
  │     err -> log tracing::warn!, rebuilt_state: None  [ADR-002]
  │
  │  [Steps 6–12 — existing, unchanged]
  │
  │  [Step 13 — existing]
  ├─ ServiceLayer::with_rate_config(...)
  │     internally: typed_graph_state = TypedGraphState::new_handle()  <- cold start
  │                 Arc::clone(&typed_graph_state) -> SearchService.typed_graph_state
  │     returns: inner: ServiceLayer
  │
  │  [Step 13b — NEW]
  ├─ if let Some(state) = rebuilt_state:
  │     handle = inner.typed_graph_handle()          <- Arc::clone of same allocation
  │     guard  = handle.write().unwrap_or_else(...)  <- write lock
  │     *guard = state                               <- swap; immediately visible to SearchService
  │     tracing::info! "eval: TypedGraphState rebuilt"  [ADR-002]
  │
  └─ Ok(EvalServiceLayer { inner, pool, embed_handle, db_path, profile_name,
                            analytics_mode: Suppressed, nli_handle })

At query time (SearchService):
  SearchService.search()
    -> read lock on SearchService.typed_graph_state (same Arc as inner.typed_graph_state)
    -> guard.use_fallback == false  <- visible because of Step 13b swap
    -> graph_expand / PPR / graph_penalty paths execute
```

---

## Shared Types (all existing, none new)

| Type | Source | Usage in crt-045 |
|------|--------|-----------------|
| `TypedGraphState` | `services/typed_graph.rs:41` | Rebuilt value; written into handle at Step 13b |
| `TypedGraphStateHandle` | `services/typed_graph.rs:161` = `Arc<RwLock<TypedGraphState>>` | Returned by new `typed_graph_handle()` accessor; write-locked at Step 13b |
| `StoreError` | `unimatrix-store` | Returned by `rebuild()` on failure; matched in Step 5b |
| `ServiceSearchParams` | `services/search.rs:256` | Constructed in test to invoke live search |
| `AuditContext` | `services/mod.rs:107` | Constructed in test to invoke live search |
| `CallerId` | `services/mod.rs:70` | Constructed in test to invoke live search |
| `AuditSource` | `services/mod.rs:116` | `Internal { service }` variant used in test |

No new structs, enums, or type aliases introduced by crt-045.

---

## Sequencing Constraints

1. **Read `services/mod.rs:399` and `:419`** before writing code — confirm `Arc::clone` at
   line 419 (SR-01). Already verified; documented in ADR-001.

2. **Step 5b precedes Step 13:** `rebuild()` is called after store construction (Step 5) and
   before `with_rate_config()` (Step 13) so the rebuilt state is ready for write-back.

3. **Step 13b follows Step 13:** The write-back requires the `ServiceLayer` to exist (to call
   `inner.typed_graph_handle()`). The lock is acquired after `with_rate_config()` returns.

4. **Test fixture order:** Insert entries first, then insert graph edges via raw SQL (edges
   reference `source_id`/`target_id` that must already be valid entry IDs in the store).
   Then dump VectorIndex (optional but required for `from_profile()` vector path).

5. **TOML fix is independent:** The `ppr-expander-enabled.toml` change has no build-time
   dependency on the Rust changes. It can be applied in any order relative to Steps 5b/13b.

---

## Integration Surface (all existing, none new in service layer)

| Interface | Signature | Source |
|-----------|-----------|--------|
| `TypedGraphState::rebuild` | `pub async fn rebuild(store: &Store) -> Result<TypedGraphState, StoreError>` | `services/typed_graph.rs:91` |
| `ServiceLayer::typed_graph_handle` | `pub fn typed_graph_handle(&self) -> TypedGraphStateHandle` | `services/mod.rs:297` |
| `TypedGraphStateHandle` | `Arc<RwLock<TypedGraphState>>` | `services/typed_graph.rs:161` |
| `SearchService::search` | `pub(crate) async fn search(&self, params: ServiceSearchParams, audit_ctx: &AuditContext, caller_id: &CallerId) -> Result<SearchResults, ServiceError>` | `services/search.rs:551` |
| `find_terminal_active` | `pub fn find_terminal_active(id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> Option<u64>` | `unimatrix_engine::graph` |

New interface introduced:
| `EvalServiceLayer::typed_graph_handle` | `pub(crate) fn typed_graph_handle(&self) -> TypedGraphStateHandle` | NEW in `eval/profile/layer.rs` |
