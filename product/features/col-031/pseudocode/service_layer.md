# col-031: services/mod.rs ServiceLayer Wiring — Pseudocode

File: `crates/unimatrix-server/src/services/mod.rs`
Status: MODIFIED

---

## Purpose

Create `PhaseFreqTableHandle` once in `ServiceLayer::with_rate_config`, thread it
into `SearchService::new`, store it in the `ServiceLayer` struct, and expose it
via a public accessor for `main.rs` to pass to `spawn_background_tick`.

The pattern is structurally identical to `typed_graph_state` (crt-021). Read that
code as the direct template.

---

## Change 1: Module Declaration

Add the new module to the `pub(crate) mod` block:

```
pub(crate) mod phase_freq_table;   // col-031: add after typed_graph
```

---

## Change 2: Re-exports

Add after the existing `TypedGraphState` / `TypedGraphStateHandle` re-export:

```
pub use phase_freq_table::{PhaseFreqTable, PhaseFreqTableHandle};
```

---

## Change 3: `ServiceLayer` Struct Field

Add `phase_freq_table` field to the `ServiceLayer` struct.
Insert after the `typed_graph_state` field and its comment block:

```
/// col-031: phase-conditioned frequency table handle shared with SearchService
/// and the background tick. Created once in with_rate_config; Arc::clone'd
/// into SearchService and exposed via phase_freq_table_handle() accessor.
/// Mirrors typed_graph_state (crt-021) and effectiveness_state (crt-018b).
phase_freq_table: PhaseFreqTableHandle,
```

---

## Change 4: `with_rate_config` — Handle Creation

In `with_rate_config`, after the line:
```rust
let typed_graph_state = TypedGraphState::new_handle();
```

Add:

```
// col-031: create phase frequency table handle once; Arc::clone into SearchService
// and expose via accessor for background tick. Mirrors typed_graph_state pattern.
let phase_freq_table = PhaseFreqTable::new_handle();
```

---

## Change 5: `SearchService::new` Call — Add Handle Argument

In `with_rate_config`, the existing `SearchService::new(...)` call currently ends with
`fusion_weights`. Add `phase_freq_table` as the last argument:

```
let search = SearchService::new(
    Arc::clone(&store),
    Arc::clone(&vector_store),
    Arc::clone(&entry_store),
    Arc::clone(&embed_service),
    Arc::clone(&adapt_service),
    Arc::clone(&gateway),
    Arc::clone(&confidence_state_handle),
    Arc::clone(&effectiveness_state),
    Arc::clone(&typed_graph_state),
    boosted_categories,
    Arc::clone(&ml_inference_pool),
    Arc::clone(&nli_handle),
    nli_top_k,
    nli_enabled,
    FusionWeights::from_config(&inference_config),
    Arc::clone(&phase_freq_table),   // col-031: required non-optional (ADR-005)
);
```

---

## Change 6: `ServiceLayer` Struct Literal — Add Field

In the `ServiceLayer { ... }` construction at the end of `with_rate_config`, add:

```
ServiceLayer {
    search,
    store_ops,
    confidence,
    briefing,
    status,
    usage,
    effectiveness_state,
    typed_graph_state,
    contradiction_cache,
    ml_inference_pool,
    phase_freq_table,    // col-031: held for external access via phase_freq_table_handle()
}
```

---

## Change 7: Accessor Method

Add `phase_freq_table_handle()` accessor to `impl ServiceLayer`.
Insert after the `typed_graph_handle()` accessor, following the same pattern:

```
/// Return a clone of the PhaseFreqTableHandle owned by this layer.
///
/// Used by the binary crate (main.rs) to pass the shared handle to
/// spawn_background_tick so the background tick rebuilds the same
/// Arc<RwLock<PhaseFreqTable>> that SearchService reads from.
/// Mirrors typed_graph_handle() (crt-021).
pub fn phase_freq_table_handle(&self) -> PhaseFreqTableHandle {
    Arc::clone(&self.phase_freq_table)
}
```

---

## Full Change Summary in `with_rate_config`

The three new lines added to `with_rate_config` (in order):

1. After `typed_graph_state = TypedGraphState::new_handle()`:
   ```
   let phase_freq_table = PhaseFreqTable::new_handle();
   ```

2. In `SearchService::new(...)` call, last argument:
   ```
   Arc::clone(&phase_freq_table),
   ```

3. In `ServiceLayer { ... }` literal:
   ```
   phase_freq_table,
   ```

---

## Error Handling

No error handling needed in this file — `PhaseFreqTable::new_handle()` cannot fail
(it allocates a cold-start `RwLock` which always succeeds). All error handling is
in `background.rs` at rebuild time.

---

## Key Test Scenarios

### AC-05: ServiceLayer creates handle and threads to SearchService

```
// Code review: confirm with_rate_config creates PhaseFreqTable::new_handle()
// and passes Arc::clone to SearchService::new as a required argument.
// Confirm ServiceLayer struct has phase_freq_table field (not Option<...>).
// Confirm accessor returns Arc::clone.
```

### R-01: No Option<PhaseFreqTableHandle> at any site

```
// Code review: grep for `Option<PhaseFreqTableHandle>` in the entire workspace.
// Must return zero results. The handle is always a required non-optional parameter
// (ADR-005). Missing wiring is a compile error.
```

### R-14: All SearchService::new call sites compile

```
// cargo build --workspace must pass after all 7 wiring sites are updated.
// The new required parameter on SearchService::new causes compile errors at any
// site that was not updated. This is the primary enforcement mechanism (ADR-005).
```
