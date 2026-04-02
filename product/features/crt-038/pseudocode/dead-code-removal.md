# Component: Dead-Code Removal (Components 3 + 4 + 5)

**Wave**: 2 (single agent — shared files across all three components)
**Files**:
  - `crates/unimatrix-server/src/services/nli_detection.rs` (Components 3 + 4)
  - `crates/unimatrix-server/src/services/store_ops.rs` (Component 3)
  - `crates/unimatrix-server/src/services/mod.rs` (Component 3)
  - `crates/unimatrix-server/src/background.rs` (Components 4 + 5)
**ACs**: AC-03, AC-04, AC-05, AC-06, AC-07, AC-08, AC-09, AC-13, AC-14
**Risks**: R-04 (High), R-05 (High), R-06 (High), R-07 (High), R-08 (Med), R-11 (Low)

---

## Purpose

Surgically delete three dead NLI code paths. Each component is independent of the
others functionally. They share files, which is why this is a single-agent wave.

The recommended edit order within Wave 2 (minimizes mid-edit compile failures):

```
1. nli_detection.rs — delete run_post_store_nli, write_edges_with_cap,
                       maybe_run_bootstrap_promotion, run_bootstrap_promotion,
                       and their 13 test functions
2. store_ops.rs    — delete NliStoreConfig struct/impl, nli_cfg field, spawn block
3. services/mod.rs — delete NliStoreConfig import and nli_store_cfg construction
4. background.rs   — delete maybe_run_bootstrap_promotion import + call site,
                       then NliQuarantineCheck enum, nli_auto_quarantine_allowed fn,
                       process_auto_quarantine NLI guard block + parameters,
                       maintenance_tick parameter strip, and 4 test functions
5. cargo build --workspace  (verify no compile errors before running tests)
6. cargo test --workspace
7. cargo clippy --workspace -- -D warnings
```

---

## Component 3: run_post_store_nli Removal

### nli_detection.rs

**Symbols to delete**:

```
pub async fn run_post_store_nli(...)      // lines ~39–195 (includes all internal steps)
pub(crate) async fn write_edges_with_cap(...)  // lines ~456–530 (sole caller was run_post_store_nli)
```

**How to identify the deletion range**:

```
run_post_store_nli:
  Starts: line 39 — pub async fn run_post_store_nli(
  Ends:   before the next pub fn/async fn/pub(crate) fn at approx line 196
  Contains: all internal logic including calls to write_edges_with_cap

write_edges_with_cap:
  Starts: line 456 — async fn write_edges_with_cap(
  Ends:   before write_nli_edge at line 532
  Note:   this function has zero callers after run_post_store_nli is deleted;
          clippy -D warnings will flag it as dead code if retained (R-05)
```

**Shared helpers that MUST NOT be deleted** (AC-13):

```
pub(crate) async fn write_nli_edge         (line 532)
pub(crate) fn format_nli_metadata          (line 628)
pub(crate) fn current_timestamp_secs       (line 639)
```

These three are imported by `nli_detection_tick.rs:34`. Deleting or making them
private causes a compile failure in `nli_detection_tick.rs`.

**Test functions to delete** (from the `#[cfg(test)]` block in nli_detection.rs):

```
test_empty_embedding_skips_nli
test_nli_not_ready_exits_immediately
test_circuit_breaker_stops_at_cap
test_circuit_breaker_counts_all_edge_types
```

These four tests exercise `run_post_store_nli` or `write_edges_with_cap` directly.

**Edge case warning — test module boundary**:

`nli_detection.rs` has 13 test functions total to delete across both Component 3 and
Component 4. They coexist in the same `#[cfg(test)] mod tests { ... }` block with any
tests to retain (if any exist for retained helpers). Use targeted function-by-function
deletion rather than deleting the entire `mod tests { }` block. Verify post-deletion
that the `#[cfg(test)]` and `mod tests {` lines are still present if any tests remain.

**Module-level doc comment**:

After deletion, update the file's top-level doc comment (`//!` block) to reflect the
new state. The file now contains only shared helpers for graph edge operations:

```
//! NLI graph edge helpers — shared utilities for NLI edge writes and metadata.
//!
//! This module provides three pub(crate) helpers consumed by `nli_detection_tick.rs`:
//! `write_nli_edge`, `format_nli_metadata`, and `current_timestamp_secs`.
//!
//! `run_post_store_nli`, `maybe_run_bootstrap_promotion`, and related functions
//! were removed in crt-038 (conf-boost-c formula migration).
//! Module merge into `nli_detection_tick.rs` is deferred to Group 2 (ADR-004).
```

### store_ops.rs

**Symbols to delete**:

```
// Import at top of file (line 20):
use crate::services::nli_detection::run_post_store_nli;

// Struct and impl (lines 33–61):
/// NliStoreConfig ...
pub(crate) struct NliStoreConfig {
    pub enabled: bool,
    pub nli_post_store_k: usize,
    pub nli_entailment_threshold: f32,
    pub nli_contradiction_threshold: f32,
    pub max_contradicts_per_tick: usize,
}
impl Default for NliStoreConfig { ... }

// Field on StoreService struct (line 103):
pub(crate) nli_cfg: NliStoreConfig,

// Comment above field (line 101-102):
/// crt-023 (ADR-004): NLI handle for post-store edge detection.   <- keep (references nli_handle)
/// crt-023: NLI inference config snapshot (avoids passing full InferenceConfig through service layer).  <- delete

// Parameter on StoreService::new (line 119):
nli_cfg: NliStoreConfig,

// Field assignment in StoreService::new body (line 132):
nli_cfg,

// tokio::spawn NLI block in insert() (lines ~303–328):
if self.nli_cfg.enabled && self.nli_handle.is_ready_or_loading() && !embedding.is_empty() {
    let embedding_for_nli = embedding;
    let entry_text_for_nli = record.content.clone();
    let nli_handle = Arc::clone(&self.nli_handle);
    let store_for_nli = Arc::clone(&self.store);
    let vector_index_for_nli = Arc::clone(&self.vector_index);
    let rayon_pool_for_nli = Arc::clone(&self.rayon_pool);
    let nli_cfg = self.nli_cfg.clone();
    tokio::spawn(async move {
        run_post_store_nli(...).await;
    });
}
// Delete entire if block (~303–328), including the surrounding comments
// "NLI hand-off point..." and "Guard: skip if NLI disabled..."
```

**After deletion**, `StoreService::new` loses one parameter. The `#[allow(clippy::too_many_arguments)]`
attribute at line 107 may still be needed (verify against the remaining parameter count
after deletion — if 10 or fewer remain, clippy may not require the allow annotation
and it can be removed; if still needed, retain it).

**StoreService struct field deletion note**: The `nli_handle` field (line 101) is NOT
deleted — it may still be used by other code paths. Only the `nli_cfg` field is deleted.
Verify `nli_handle` has callers after deletion before considering removal; it is out of
scope for this feature.

### services/mod.rs

**Symbols to delete**:

```
// Import at line 26:
use crate::services::store_ops::NliStoreConfig;

// Construction block at lines ~435–441:
let nli_store_cfg = NliStoreConfig {
    enabled: inference_config.nli_enabled,
    nli_post_store_k: inference_config.nli_post_store_k,
    nli_entailment_threshold: inference_config.nli_entailment_threshold,
    nli_contradiction_threshold: inference_config.nli_contradiction_threshold,
    max_contradicts_per_tick: inference_config.max_contradicts_per_tick,
};

// Argument to StoreService::new at line ~454:
nli_store_cfg,    // <- delete this argument from the call
```

After deletion, the `StoreService::new(...)` call will have one fewer argument,
matching the updated signature in `store_ops.rs`.

---

## Component 4: maybe_run_bootstrap_promotion Removal

### nli_detection.rs (continued from Component 3)

**Symbols to delete**:

```
pub async fn maybe_run_bootstrap_promotion(...)  // lines ~197–236
async fn run_bootstrap_promotion(...)            // lines ~237–455
```

These functions are adjacent. `run_bootstrap_promotion` is the private implementation
called only by `maybe_run_bootstrap_promotion`.

**Test functions to delete** (from the same `#[cfg(test)]` block):

```
test_bootstrap_promotion_zero_rows_sets_marker
test_maybe_bootstrap_promotion_skips_if_marker_present
test_maybe_bootstrap_promotion_defers_when_nli_not_ready
test_bootstrap_promotion_confirms_above_threshold
test_bootstrap_promotion_refutes_below_threshold
test_bootstrap_promotion_idempotent_second_run_no_duplicates
test_bootstrap_promotion_nli_inference_runs_on_rayon_thread
```

Seven test functions. Together with the four from Component 3, this accounts for 11
of the 13 total test functions to delete. The remaining 2 are listed in the spec as
"covering `run_post_store_nli`" — grep for any additional test functions referencing
`run_post_store_nli` or `write_edges_with_cap` to confirm all 13 are deleted.

Delivery must run:
```
grep -c "fn test_" crates/unimatrix-server/src/services/nli_detection.rs
```
before and after deletion to confirm the count drops by exactly 13 (or to zero if no
other tests exist in this file). If the module has tests for the retained helpers,
those must not be deleted.

### background.rs (Component 4 edits)

**Symbols to delete**:

```
// Import at line 49:
use crate::services::nli_detection::maybe_run_bootstrap_promotion;

// Call site block at lines ~772–777:
// crt-023: Bootstrap NLI promotion (one-shot, idempotent via COUNTERS marker).
// Called on every tick; fast no-op if marker already set (O(1) DB read).
// When NLI is not ready, defers silently without setting marker (FR-25).
if inference_config.nli_enabled {
    maybe_run_bootstrap_promotion(store, nli_handle, ml_inference_pool, inference_config).await;
}

// Stale sequencing comment at line ~781 (R-11):
// Runs after bootstrap promotion so bootstrap-promoted edges are visible to the tick's
// pre-filter HashSet. Must remain after maybe_run_bootstrap_promotion (sequencing invariant).
```

The stale sequencing comment ("Must remain after maybe_run_bootstrap_promotion") must
be removed or rewritten. The `run_graph_inference_tick` call that follows at line ~782
is retained (Group 2). Its sequencing relative to bootstrap promotion no longer applies.
If any sequencing comment is still meaningful (e.g., relative to confidence state or
effectiveness state), rewrite it. If not meaningful, remove entirely.

**The `if inference_config.nli_enabled { run_graph_inference_tick(...) }` block at
line ~782 is NOT deleted.** Only the bootstrap promotion block (lines 772–777) is
deleted. The inference tick block is retained.

---

## Component 5: NLI Auto-Quarantine Guard Removal

### background.rs (continued from Component 4)

**Symbols to delete**:

```
// Enum at lines ~1233–1241:
#[derive(Debug)]
enum NliQuarantineCheck {
    Allowed,
    BlockedBelowThreshold,
    StoreError(unimatrix_store::StoreError),
}

// Private function at lines ~1254–1291:
async fn nli_auto_quarantine_allowed(
    store: &Arc<Store>,
    entry_id: u64,
    threshold: f32,
) -> NliQuarantineCheck { ... }

// Comment block above nli_auto_quarantine_allowed and the enum (lines ~1227–1230):
// ---------------------------------------------------------------------------
// NLI auto-quarantine threshold guard (crt-023, ADR-007)
// ---------------------------------------------------------------------------
// Delete the section header comment along with the enum and function.
```

**process_auto_quarantine — parameter strip and body edit**:

```
// Before signature (lines 1090–1099):
async fn process_auto_quarantine(
    to_quarantine: Vec<(u64, u32, EffectivenessCategory)>,
    effectiveness_state: &EffectivenessStateHandle,
    effectiveness_report: &unimatrix_engine::effectiveness::EffectivenessReport,
    store: &Arc<Store>,
    audit_log: &Arc<AuditLog>,
    auto_quarantine_cycles: u32,
    nli_enabled: bool,                      // DELETE this param + comment
    nli_auto_quarantine_threshold: f32,     // DELETE this param + comment
) -> Vec<u64>

// After signature:
async fn process_auto_quarantine(
    to_quarantine: Vec<(u64, u32, EffectivenessCategory)>,
    effectiveness_state: &EffectivenessStateHandle,
    effectiveness_report: &unimatrix_engine::effectiveness::EffectivenessReport,
    store: &Arc<Store>,
    audit_log: &Arc<AuditLog>,
    auto_quarantine_cycles: u32,
) -> Vec<u64>

// Body: delete the NLI guard block at lines ~1115–1145:
// Delete from:
//   // crt-023 ADR-007: NLI auto-quarantine threshold guard.
// Through (inclusive):
//   }
// The outer `}` closing the `if nli_enabled { ... }` block.

// After deletion, the loop body for each entry_id goes directly from:
//   category check (lines ~1110–1113)
//   to:
//   fetch entry metadata (lines ~1147–1155)
// with no NLI guard in between.
```

**Comment cleanup in process_auto_quarantine**:

The comments explaining the NLI guard logic (lines ~1115–1123) are part of the deleted
block and should be deleted with it:
```
// crt-023 ADR-007: NLI auto-quarantine threshold guard.
// Before quarantining, check if the topology penalty...
// ... (all related comment lines through line ~1123)
// Guard skipped when nli_enabled=false (never writes NLI edges).
```

**Call site update — maintenance_tick (line ~946)**:

```
// Before:
let quarantined_ids = process_auto_quarantine(
    to_quarantine,
    effectiveness_state,
    effectiveness_report,
    store,
    audit_log,
    auto_quarantine_cycles,
    nli_enabled,                      // DELETE this argument
    nli_auto_quarantine_threshold,    // DELETE this argument
)
.await;

// After:
let quarantined_ids = process_auto_quarantine(
    to_quarantine,
    effectiveness_state,
    effectiveness_report,
    store,
    audit_log,
    auto_quarantine_cycles,
)
.await;
```

**Cascade: nli_enabled and nli_auto_quarantine_threshold propagation**:

These two values are passed through a chain of function signatures:
```
spawn_background_tick (outer) -- lines 250-251
  -> background_tick_loop      -- lines 327-328
    -> run_single_tick         -- lines 441-442
      -> maintenance_tick      -- lines 820-821
        -> process_auto_quarantine -- lines 1097-1098 (DELETED from here)
```

Delivery must strip the parameters from ALL four callers in this chain:
- `spawn_background_tick` function parameter list (lines 250-251)
- `background_tick_loop` function parameter list (lines 327-328) + pass-through call
- `run_single_tick` function parameter list (lines 441-442) + pass-through call
- `maintenance_tick` function parameter list (lines 820-821) + call site at line ~946

**Call site of spawn_background_tick** (wherever it is called in the server startup
code): also remove the two arguments passed there.

Grep for the call site:
```
grep -n "spawn_background_tick" crates/unimatrix-server/src/
```

Each occurrence must drop `nli_enabled` and `nli_auto_quarantine_threshold` arguments.

**Test functions to delete** (from `#[cfg(test)]` in background.rs):

```
test_nli_edges_below_auto_quarantine_threshold_no_quarantine
test_nli_edges_above_threshold_allow_quarantine
test_nli_auto_quarantine_mixed_penalty_allowed
test_nli_auto_quarantine_no_edges_allowed
```

These four tests are integration tests (annotated `#[tokio::test(flavor = "multi_thread")]`)
that test `nli_auto_quarantine_allowed` directly. They are in the block labeled
"nli_auto_quarantine_allowed integration tests" (line ~3360).

Also delete: the section comment header "nli_auto_quarantine_allowed integration tests"
at approximately line 3360.

`parse_nli_contradiction_from_metadata` (line ~1297) is a private helper used only by
`nli_auto_quarantine_allowed`. After deleting `nli_auto_quarantine_allowed`, this
function becomes callerless dead code. Delete it too. Any tests for it (in the test
block near line ~3355) must also be deleted.

Grep to confirm:
```
grep -n "parse_nli_contradiction_from_metadata" crates/unimatrix-server/src/background.rs
```
All occurrences must be deleted (the function definition and all call sites).

---

## Grep Verification Checklist (run before marking ACs complete)

```bash
# Component 3 (AC-03, AC-04, AC-14):
grep -r "run_post_store_nli" crates/             # must return 0 matches
grep -r "NliStoreConfig" crates/                 # must return 0 matches
grep -r "nli_store_cfg" crates/                  # must return 0 matches
grep -r "write_edges_with_cap" crates/           # must return 0 matches (R-05)

# Component 4 (AC-05, AC-06):
grep -r "maybe_run_bootstrap_promotion" crates/  # must return 0 matches
grep -r "run_bootstrap_promotion" crates/        # must return 0 matches

# Component 5 (AC-07, AC-08):
grep -r "nli_auto_quarantine_allowed" crates/    # must return 0 matches
grep -r "NliQuarantineCheck" crates/             # must return 0 matches
grep -r "parse_nli_contradiction_from_metadata" crates/  # must return 0 matches

# Retained symbols (AC-13 — must still exist):
grep -n "pub(crate) async fn write_nli_edge" crates/unimatrix-server/src/services/nli_detection.rs
grep -n "pub(crate) fn format_nli_metadata" crates/unimatrix-server/src/services/nli_detection.rs
grep -n "pub(crate) fn current_timestamp_secs" crates/unimatrix-server/src/services/nli_detection.rs
# Each must return exactly 1 match (the definition)
```

---

## Error Handling

No new error handling is introduced by these deletions. The removed code paths either
were fire-and-forget (`tokio::spawn`) or returned `Result`/unit values that were
discarded by callers. After deletion:

- `store_ops.rs:insert()` no longer spawns the NLI task; no error propagation needed.
- `background.rs:process_auto_quarantine()` no longer calls `nli_auto_quarantine_allowed`;
  quarantine proceeds directly without the NLI guard check.
- Callers of `process_auto_quarantine` lose two arguments; `Result` return type is
  unchanged (still `Vec<u64>`).

---

## Key Test Scenarios Summary

### Compile Verification (AC-13)

| Check | Expected |
|-------|----------|
| `cargo build --workspace` after all deletions | Zero errors |
| `nli_detection_tick.rs` imports compile | Three retained symbols still present as `pub(crate)` |
| `store_ops.rs:StoreService::new` compiles | One fewer parameter than before |
| `background.rs:process_auto_quarantine` compiles | Two fewer parameters than before |
| `background.rs:maintenance_tick` call site | Drops two arguments, matches new signature |

### Dead Code Elimination (AC-11)

| Check | Expected |
|-------|----------|
| `cargo clippy --workspace -- -D warnings` | Zero warnings |
| `write_edges_with_cap` absent from compiled source | Zero grep matches |
| `NliQuarantineCheck` absent from compiled source | Zero grep matches |

### Test Deletion (AC-09)

| File | Tests deleted | Tests retained |
|------|--------------|----------------|
| `nli_detection.rs` | 13 (see spec) | 0 or more tests for retained helpers |
| `background.rs` | 4 + any for `parse_nli_contradiction_from_metadata` | All other tests |
