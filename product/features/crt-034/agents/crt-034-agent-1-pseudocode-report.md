# Agent Report: crt-034-agent-1-pseudocode

## Task

Produce per-component pseudocode files for crt-034 (recurring co_access → GRAPH_EDGES
promotion tick). Four components: store_constants, config_extension,
co_access_promotion_tick, background_tick_insertion.

## Output Files

- `product/features/crt-034/pseudocode/OVERVIEW.md`
- `product/features/crt-034/pseudocode/store_constants.md`
- `product/features/crt-034/pseudocode/config_extension.md`
- `product/features/crt-034/pseudocode/co_access_promotion_tick.md`
- `product/features/crt-034/pseudocode/background_tick_insertion.md`

## Components Covered

1. **store_constants** — Two new public constants in `unimatrix-store/src/read.rs` +
   re-export in `lib.rs`. No new files. CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3 and
   EDGE_SOURCE_CO_ACCESS: &str = "co_access".

2. **config_extension** — `max_co_access_promotion_per_tick: usize` field in
   `InferenceConfig` with serde default fn (200), validate() range [1, 10000],
   Default impl stanza, and merge_configs() stanza. Mirrors max_graph_inference_per_tick
   pattern exactly.

3. **co_access_promotion_tick** — New module `services/co_access_promotion_tick.rs`.
   Full algorithmic pseudocode for `run_co_access_promotion_tick`, including Phase 1
   (batch fetch with embedded scalar subquery MAX), Phase 2 (max_count extraction and
   zero-guard), Phase 3 (per-pair two-step INSERT OR IGNORE + delta-guarded UPDATE),
   Phase 4 (summary info! log). All error paths, SR-05 warn, and 14 named test scenarios.

4. **background_tick_insertion** — Wiring in `background.rs`: PROMOTION_EARLY_RUN_WARN_TICKS
   constant, import addition, current_tick counter note, call site insertion with ORDERING
   INVARIANT anchor comment block.

## Open Questions Found

**OQ-1: `current_tick` loop counter in `run_background_tick`**: The `run_single_tick`
function signature already declares `current_tick: u32` as a parameter (confirmed at
line ~431 of background.rs). However, the calling loop (`run_background_tick`) was not
fully read to confirm the counter is already tracked and incremented there. The
implementation agent must verify: (a) whether the loop already has a `current_tick`
counter and saturation increment, or (b) whether this counter needs to be added. The
pseudocode handles both cases.

**OQ-2: `inference_config` type at call site**: In `run_single_tick`, `inference_config`
is threaded as an `Arc<InferenceConfig>`. The call to `run_co_access_promotion_tick`
expects `&InferenceConfig`. The implementation agent should use `inference_config.as_ref()`
or `&**inference_config` depending on the exact type in scope at the insertion point.
This is a minor implementation detail, not a design issue.

**OQ-3: migration constant alignment (AC-07)**: `CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3` in
`migration.rs` is file-private and not in scope to change (explicitly out of scope per
IMPLEMENTATION-BRIEF). The pseudocode documents this and calls for a code-review note at
delivery confirming both constants equal 3. No structural enforcement is possible without
changing migration.rs.

**OQ-4: GH #409 sequencing**: Has GH #409 been merged? If yes, zero qualifying rows on
first run after deployment means SR-05 warn will fire — which is the intended signal-loss
detection behavior. Verify at delivery gate.

## Deviations from Established Patterns

- **CO_ACCESS_WEIGHT_UPDATE_DELTA type**: ARCHITECTURE.md integration surface table lists
  this as `const f32 = 0.1`. The IMPLEMENTATION-BRIEF (ADR-003) resolves this to `f64 = 0.1`
  to avoid sqlx REAL→f64 precision noise. The pseudocode follows the IMPLEMENTATION-BRIEF
  resolution (f64), which is authoritative. The architecture integration surface table is
  considered superseded on this point.

- **Function signature `current_tick` parameter**: ARCHITECTURE.md integration surface
  table lists `async fn(store: &Store, config: &InferenceConfig)` (two parameters). The
  IMPLEMENTATION-BRIEF resolves this to three parameters including `current_tick: u32`
  (required for SR-05 warn). The pseudocode follows the IMPLEMENTATION-BRIEF resolution.

Both deviations are documented in OVERVIEW.md.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` — pattern/background tick — found #3822
  (near-threshold idempotency pattern) and #3821 (GRAPH_EDGES tick write path + ordering
  pattern). Both directly incorporated into co_access_promotion_tick.md error handling
  and AC-14/AC-15 scenarios.
- Queried: `mcp__unimatrix__context_search` — decision/crt-034 — found #3829 (ADR-003
  f64 delta), #3827 (ADR-005 tick insertion + SR-05), #3826 (ADR-004 InferenceConfig
  field), #3830 (ADR-006 directionality). All four applied to pseudocode.
- Queried: `mcp__unimatrix__context_briefing` — confirmed #3821, #3822, #3824, #3826,
  #3827. Also surfaced #2704 (bootstrap idempotency via COUNTERS) and #3656 (module size
  split pattern) — both referenced in background_tick_insertion.md and co_access_promotion_tick.md.
- Deviations from established patterns: CO_ACCESS_WEIGHT_UPDATE_DELTA is f64 not f32
  (deviation from architecture surface table, authorized by IMPLEMENTATION-BRIEF/ADR-003).
  Function has 3 parameters not 2 (authorized by IMPLEMENTATION-BRIEF/ADR-005). Both
  documented above.
