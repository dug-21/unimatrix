## ADR-005: Tick Insertion Point — Anchor Comment + SR-05 Early-Run Detectability

### Context

#### Tick ordering

The promotion tick must run AFTER `maintenance_tick` (which calls
`cleanup_stale_co_access`) and AFTER orphaned-edge compaction, but BEFORE
`TypedGraphState::rebuild()`, so freshly promoted edges are immediately visible to PPR
in the same tick cycle (SCOPE.md §Goal 5, §Background Research — Tick Ordering).

The current sequence in `run_single_tick` (background.rs lines ~500-784):

```
(1) maintenance_tick()
(2) GRAPH_EDGES orphaned-edge compaction  [lines ~500-547]
(3) TypedGraphState::rebuild()            [lines ~549-594]
(4) PhaseFreqTable::rebuild()
(5) Contradiction scan
(6) extraction_tick()
(7) maybe_run_bootstrap_promotion()       [NLI, one-shot]
(8) run_graph_inference_tick()            [NLI, recurring]
```

SR-06 (SCOPE-RISK-ASSESSMENT.md) notes that future tick steps added between steps 2 and
3 by concurrent features could inadvertently push promotion after rebuild.

Two options for making the insertion point stable:

**Option A — Named comment anchor**: Add a structured comment block at the exact
insertion point. The anchor comment is a contract that future developers must read before
adding tick steps in that region.

**Option B — Explicit ordering constant or enum**: Define a sequence as a named enum or
static array (e.g., `TickPhase`) to express ordering programmatically.

Option B adds runtime indirection for what is fundamentally a sequential execution
sequence. The existing tick is a series of `match` blocks and function calls; adding an
enum-driven dispatcher would require significant refactoring with no behavioral benefit.
The codebase convention is anchor comments (see existing `// crt-021 Step 2:` comment at
line 500).

#### SR-05: First-run signal-loss detectability

SR-05 is rated High severity: if GH #409 ships before crt-034 and prunes co_access rows,
qualifying pairs are silently lost. The promotion tick returns zero rows from its batch
query with no error.

Two detectability options:

**Option A — Unconditional tick, zero-row warn on first N ticks**: If
`qualifying_pair_count == 0` AND `current_tick < PROMOTION_EARLY_RUN_WARN_TICKS`, emit
`tracing::warn!` to surface possible signal loss. After that window the zero-row case is
expected (all pairs already promoted).

**Option B — COUNTERS marker tracking first promotion**: Write a COUNTERS marker on the
first successful promotion, then check the marker each tick to distinguish "no rows yet"
from "all rows already promoted." This is the pattern from crt-023 (bootstrap NLI).

Option B is rejected because this tick is explicitly recurring (SCOPE.md §Design Decision
5 — No COUNTERS marker). A COUNTERS marker implies one-shot semantics. The tick must
re-examine `co_access` every cycle to catch new pairs that cross the threshold.

The `current_tick` parameter is already threaded into `run_single_tick`. It is NOT
threaded into `run_co_access_promotion_tick` — the function signature stays minimal
(`store: &Store, config: &InferenceConfig`). The early-run check belongs in the
`background.rs` call site, not inside the tick function.

### Decision

**Tick insertion**: Insert the `run_co_access_promotion_tick` call with an anchor comment
immediately after the orphaned-edge compaction block (line ~547) and before the
`TypedGraphState::rebuild()` block (line ~549):

```rust
// crt-034: Co-access promotion tick.
//
// Promotes qualifying co_access pairs (count >= CO_ACCESS_GRAPH_MIN_COUNT) into
// GRAPH_EDGES as CoAccess edges, and refreshes weights on existing edges that have
// drifted by more than CO_ACCESS_WEIGHT_UPDATE_DELTA.
//
// ORDERING INVARIANT (SR-06):
//   MUST run AFTER maintenance_tick (step 1) — stale co_access rows already cleaned.
//   MUST run AFTER orphaned-edge compaction (step 2) — dead-endpoint edges removed.
//   MUST run BEFORE TypedGraphState::rebuild() (step 3) — so promoted edges are
//   visible to PPR in the same tick cycle.
//
// Unconditional: does not require NLI. Pure SQL, infallible, no rayon pool.
run_co_access_promotion_tick(store, inference_config).await;
```

**SR-05 detectability**: Add a `warn!` log in `background.rs` at the call site using
`current_tick`:

```rust
// SR-05: warn on first few ticks if co_access has no qualifying pairs.
// Surfaces silent signal loss if GH #409 shipped before crt-034 and pruned
// rows before the first promotion ran.
const PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5;
run_co_access_promotion_tick(store, inference_config).await;
// Note: run_co_access_promotion_tick is infallible and logs its own counts.
// The SR-05 guard below is separate: it checks tick index, not the promotion result.
if current_tick < PROMOTION_EARLY_RUN_WARN_TICKS {
    // Detectability is inside run_co_access_promotion_tick: it logs at warn!
    // when qualifying_count == 0 on ticks < PROMOTION_EARLY_RUN_WARN_TICKS.
}
```

In practice the SR-05 warn is emitted from within `run_co_access_promotion_tick` itself,
which receives `current_tick` as an additional parameter when `current_tick` is needed.

Revised function signature (SR-05 support):
```rust
pub(crate) async fn run_co_access_promotion_tick(
    store: &Store,
    config: &InferenceConfig,
    current_tick: u32,
)
```

Inside the function: if `qualifying_count == 0 && current_tick < 5`, emit:
```rust
tracing::warn!(
    current_tick,
    "co_access promotion: no qualifying pairs found on early tick; \
     if GH #409 ran before crt-034, co_access signal may have been pruned permanently"
);
```

### Consequences

- The anchor comment makes the ordering invariant explicit and machine-grep-able
  (`SR-06` in the comment body).
- The `current_tick` parameter adds one usize to the function signature. This is a minor
  deviation from the minimal signature ideal but is necessary to implement SR-05 without
  an external state flag.
- PROMOTION_EARLY_RUN_WARN_TICKS = 5 means the warn fires on ticks 0–4 (first ~75
  minutes at a 15-minute tick interval). After that, zero qualifying pairs is expected
  (all pairs already promoted).
- The COUNTERS-marker approach (Option B) is explicitly rejected. Any future developer
  who reads the anchor comment will understand why.
