# background_tick_insertion — Pseudocode

## Component: `background.rs` — tick loop wiring

### Purpose

Wire `run_co_access_promotion_tick` into `run_single_tick` at the correct position in the
tick sequence (ADR-005, #3827). Add the `PROMOTION_EARLY_RUN_WARN_TICKS` constant.
Import the new module function. Pass `current_tick` to the call site.

This component involves zero algorithmic logic — it is purely structural: constant
declaration, import, call site insertion, and the ORDERING INVARIANT anchor comment.

---

## File to Modify

**`crates/unimatrix-server/src/background.rs`**

---

## Modification 1: Add `PROMOTION_EARLY_RUN_WARN_TICKS` constant

**Location**: In the module-level constants block at the top of background.rs (near
`SYSTEM_AGENT_ID`, `OP_AUTO_QUARANTINE`, `AUTO_QUARANTINE_CYCLES_MAX`, etc.).

```
// Add alongside other tick-level constants:

/// Number of initial ticks in which qualifying_count == 0 triggers a warn!
/// log (SR-05 early-run signal-loss detectability, ADR-005 crt-034).
///
/// After this many ticks, zero qualifying pairs is silently a no-op (the table
/// may genuinely be empty or all pairs are already promoted). Before this window,
/// zero qualifying pairs may indicate that GH #409 pruned co_access before
/// crt-034 deployed.
///
/// Value 5 covers ~75 minutes of tick interval (15 min × 5) — long enough for
/// a freshly deployed server to complete initial promotion of all qualifying pairs.
const PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5;
```

---

## Modification 2: Add import for `run_co_access_promotion_tick`

**Location**: In the existing `use crate::services::...` import block (near lines 48-50
where `maybe_run_bootstrap_promotion` and `run_graph_inference_tick` are imported).

```
// Current imports in this block:
use crate::services::nli_detection::maybe_run_bootstrap_promotion;
use crate::services::nli_detection_tick::run_graph_inference_tick;

// Add:
use crate::services::co_access_promotion_tick::run_co_access_promotion_tick;
```

---

## Modification 3: `current_tick` counter in `run_background_tick` loop

**Context**: `run_single_tick` already receives `current_tick: u32` as a parameter (visible
in the function signature at line ~431). Verify whether the loop in the `run_background_tick`
task already tracks and increments this counter.

**If `current_tick` counter already exists in the loop**: No change needed — it will be
passed through to `run_co_access_promotion_tick` via `run_single_tick`.

**If `current_tick` does NOT exist in the loop**: Add it as a local `u32` counter,
initialized to 0, incremented with saturation after each `run_single_tick` call:

```
let mut current_tick: u32 = 0;

loop {
    // ... tick delay, timeout, etc. ...
    let tick_result = run_single_tick(
        ...,
        current_tick,
        ...,
    ).await;

    current_tick = current_tick.saturating_add(1);
    // saturating_add prevents overflow on very long-running processes;
    // after u32::MAX ticks the SR-05 window (< 5) will never re-open.
}
```

Note: Check the existing `run_background_tick` loop body. The `current_tick: u32` parameter
already appears in `run_single_tick`'s signature (verified at line ~431 in the file).
Trace whether it flows from the loop or is currently hardcoded. If the counter exists,
only the call-site pass-through to `run_co_access_promotion_tick` needs to be added.

---

## Modification 4: Call site insertion in `run_single_tick`

**This is the core insertion.** The exact position is defined by ADR-005:
- AFTER the orphaned-edge compaction block (lines 500-547 in background.rs)
- BEFORE the `TypedGraphState::rebuild()` block (lines 549-594 in background.rs)

**Current sequence** (simplified):
```
// ... (line 500) crt-021 Step 2: GRAPH_EDGES orphaned-edge compaction
{
    let compaction_result = sqlx::query("DELETE FROM graph_edges ...").execute(...).await;
    match compaction_result { ... }
}  // end orphaned-edge compaction block (line ~547)

// crt-021: Rebuild typed graph state after maintenance tick completes.  (line 549)
{
    let store_clone = Arc::clone(store);
    match tokio::time::timeout(..., TypedGraphState::rebuild(...)).await { ... }
}  // end TypedGraphState rebuild block
```

**Modified sequence** — insert between the two blocks:

```
// ... orphaned-edge compaction block (unchanged) ...
}  // end orphaned-edge compaction block

// -----------------------------------------------------------------------
// ORDERING INVARIANT (crt-034, ADR-005, SR-06):
//
// run_co_access_promotion_tick MUST remain between:
//   [before]  crt-021 Step 2 GRAPH_EDGES orphaned-edge compaction
//   [after]   TypedGraphState::rebuild()
//
// Rationale:
//   - Stale co_access rows are already cleaned by maintenance_tick() (step 1).
//   - Orphaned-edge compaction (step 2) removes edges with deleted endpoints.
//   - The promotion tick runs AFTER compaction so it sees the post-compaction
//     graph_edges state (avoids promoting pairs into orphan territory).
//   - TypedGraphState::rebuild() reads graph_edges AFTER promotion, so freshly
//     promoted CoAccess edges are visible to PPR in this same tick cycle.
//
// DO NOT insert new tick steps between this comment block and
// TypedGraphState::rebuild() without updating this invariant comment.
// -----------------------------------------------------------------------
// crt-034: co_access → GRAPH_EDGES promotion (recurring, cap-throttled).
// Unconditional — not gated on nli_enabled or any feature flag (FR-07).
run_co_access_promotion_tick(store, inference_config, current_tick).await;

// crt-021: Rebuild typed graph state after maintenance tick completes. (unchanged)
{
    let store_clone = Arc::clone(store);
    match tokio::time::timeout(..., TypedGraphState::rebuild(...)).await { ... }
}
```

**Key properties of the call site**:
- Unconditional: no `if inference_config.nli_enabled` guard (unlike `run_graph_inference_tick`)
- `store` is `&Arc<Store>` in scope at this point — pass directly (matches `&Store` param via Deref)
- `inference_config` is `&Arc<InferenceConfig>` or `&InferenceConfig` — verify exact type in scope; pass accordingly
- `current_tick` is the `u32` counter threaded through from the loop

**Call signature to emit**:
```
run_co_access_promotion_tick(store, inference_config, current_tick).await;
```

Where:
- `store: &Store` — resolved from `&Arc<Store>` via `Deref` auto-deref or `store.as_ref()`
- `inference_config: &InferenceConfig` — resolved from `Arc<InferenceConfig>` via `&**inference_config` or `inference_config.as_ref()`
- `current_tick: u32` — the mutable loop counter

---

## `run_single_tick` Parameter Audit

Verify before implementing that `run_single_tick` already receives:
- `store: &Arc<Store>` — confirmed at line ~416
- `inference_config: Arc<InferenceConfig>` — confirmed (used to pass to `run_graph_inference_tick` at line ~767)
- `current_tick: u32` — confirmed at line ~431 (the function signature already has this param)

No new parameters added to `run_single_tick`. All required values are already in scope.

---

## Data Flow

```
run_background_tick (loop)
  │  current_tick: u32 (starts 0, saturating_add(1) each loop)
  │
  ▼
run_single_tick(store, ..., inference_config, ..., current_tick, ...)
  │
  ├─ (step 1) maintenance_tick()   -- cleans stale co_access rows
  │
  ├─ (step 2) orphaned-edge compaction
  │
  ├─ (step 2b) ORDERING INVARIANT anchor comment
  │            run_co_access_promotion_tick(store, inference_config, current_tick).await
  │              │
  │              └─ Reads: co_access (write_pool_server)
  │              └─ Writes: graph_edges (write_pool_server)
  │              └─ Emits: warn! (SR-05, errors) + info! (summary)
  │
  └─ (step 3) TypedGraphState::rebuild()   -- sees freshly promoted edges
```

---

## Error Handling

None at this layer. `run_co_access_promotion_tick` is infallible (`-> ()`). The call site
does not inspect a return value, does not wrap in `?`, does not log errors (the function
logs its own errors). This matches the pattern for `run_graph_inference_tick` and
`maybe_run_bootstrap_promotion`.

---

## Key Test Scenarios

**AC-05** (ordering verification — static): Code review of background.rs confirms the
`run_co_access_promotion_tick` call appears between the orphaned-edge compaction block
(the closing `}` of the DELETE block) and the `// crt-021: Rebuild typed graph state`
comment introducing the TypedGraphState rebuild block.

**AC-05** (ordering verification — structural integration test, optional): If a test
harness exists that exercises the full tick sequence against a real store, seed a
qualifying co_access pair, fire one tick, and assert the pair appears in TypedRelationGraph
state (not deferred to the next tick). This confirms the promotion result is visible to
PPR within the same tick cycle.

**R-05** (tick ordering): The ORDERING INVARIANT block comment is the primary structural
guard. Any implementor reading `background.rs` will see the anchor comment before the
TypedGraphState rebuild. The comment explicitly states the DO NOT clause for future
step insertions.

**FR-07** (unconditional execution): The call site has no `if inference_config.nli_enabled`
or any other conditional guard. Confirm by code review that the call is at the top level
of `run_single_tick`, not nested inside an `if` block.

**R-12** (file size): `background.rs` is already large. The additions are:
- 1 module-level constant (~8 lines with doc comment)
- 1 import line
- 1 `.await` call line + anchor comment block (~15 lines)
Total addition: ~25 lines. No file size concern for background.rs.

**Compile check**: After insertion, `cargo check -p unimatrix-server` must succeed.
The import of `run_co_access_promotion_tick` must resolve (depends on `services/mod.rs`
registration being in place).
