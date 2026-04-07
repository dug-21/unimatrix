# Architecture: crt-050 Phase-Conditioned Category Affinity (Explicit Read Source)

GH Issue: #542

## System Overview

`PhaseFreqTable` is an in-memory tick-rebuilt cache that scores how strongly each
`(phase, category)` pair is associated with a given entry. It is used in two
scoring paths: fused re-ranking (guarded on `use_fallback`) and PPR
personalization (direct call returning neutral `1.0` on cold-start).

Today the table is populated from `query_log` search exposures — entries that
appeared in a search result set. crt-049 (#539) shipped a cleaner signal:
`observations.input` now carries the entry ID for every `context_get` and
single-ID `context_lookup` `PreToolUse` event, and `observations.phase` records
the active workflow phase at write time. crt-050 replaces the rebuild source
with this explicit read signal and adds outcome weighting from `cycle_events`.

The change is contained to:
1. Two new store query functions replacing `query_phase_freq_table`.
2. One update to `PhaseFreqTable::rebuild()` — the query call and a new
   Rust post-process weighting step.
3. One new public accessor `phase_category_weights()` on `PhaseFreqTable`.
4. A field rename in `InferenceConfig` with serde backward-compat alias.
5. Updates to the crt-036 diagnostic warning in `status.rs`.

All existing contracts (`phase_affinity_score()`, `use_fallback`, rank-based
normalization, retain-on-error, poison recovery) are preserved unchanged.

## Component Breakdown

### unimatrix-store — `src/query_log.rs` (or new `src/phase_freq.rs`)

**Changed:** Delete `query_phase_freq_table`. Add two new async functions:

- `query_phase_freq_observations(lookback_days: u32) -> Result<Vec<PhaseFreqRow>>`
  — Query A: aggregate `(phase, category, entry_id, freq)` from `observations`
  joined to `entries`. Returns the same `Vec<PhaseFreqRow>` type. `freq` is a
  raw explicit-read count at this stage (outcome weighting applied in Rust).

- `query_phase_outcome_map() -> Result<Vec<PhaseOutcomeRow>>`
  — Query B: returns `(phase, feature_cycle, outcome)` tuples from
  `cycle_events` joined to `sessions`. Used only as input to the Rust
  outcome-weighting step.

**Retained:** `PhaseFreqRow` struct and `row_to_phase_freq_row` deserializer
are unchanged in shape and re-export location. A new `PhaseOutcomeRow` struct
is added for Query B output.

**Module placement decision:** Keep both new functions in `query_log.rs` or
move them to a new `phase_freq.rs` module — implementation-time discretion.
The `PhaseFreqRow` re-export from the crate root must not change regardless of
module placement (no callers may break).

### unimatrix-server — `src/services/phase_freq_table.rs`

**Changed:**
- `PhaseFreqTable::rebuild()`: replace the single `store.query_phase_freq_table()`
  call with two calls (Query A + Query B). Add a Rust post-process weighting
  step between the two queries. All other rebuild logic (grouping, rank
  normalization, `use_fallback`) is unchanged.
- Add `phase_category_weights(&self) -> HashMap<(String, String), f32>`: new
  public method for W3-1 GNN cold-start consumption (see Integration Surface).

**Unchanged:** `PhaseFreqTable` struct fields, `phase_affinity_score()`,
`new()`, `new_handle()`, `Default`, `PhaseFreqTableHandle` type alias, poison
recovery pattern, existing tests.

### unimatrix-server — `src/infra/config.rs`

**Changed:**
- Rename `InferenceConfig::query_log_lookback_days` to `phase_freq_lookback_days`.
- Add `#[serde(alias = "query_log_lookback_days")]` for backward compatibility.
- Update `default_query_log_lookback_days()` function name to
  `default_phase_freq_lookback_days()` (or retain old name as an alias — impl
  discretion).
- Update validation error message and field name string.
- Update merge logic in config merging code.

**SR-04 surface:** All test fixtures that construct `InferenceConfig` using
struct literal syntax with `query_log_lookback_days` will fail to compile after
the rename. The implementer must audit all literal constructions in test code and
update them. The serde alias handles TOML deserialization only, not Rust struct
literal syntax.

### unimatrix-server — `src/services/status.rs`

**Changed:**
- `warn_phase_freq_lookback_mismatch()` (formerly `warn_query_log_lookback_*`):
  update function name and all field references from `query_log_lookback_days`
  to `phase_freq_lookback_days`. Update warning message text.
- Add `warn_observations_coverage()`: new tick-time diagnostic function. When
  the distinct `(phase, session)` count within the lookback window falls below a
  configurable minimum threshold (`min_phase_session_pairs`, added to
  `InferenceConfig`), emit `tracing::warn!` with the count and the threshold.

### unimatrix-server — `src/background.rs`

**Changed:** Update the field access from
`inference_config.query_log_lookback_days` to
`inference_config.phase_freq_lookback_days` (one site, confirmed at grep line 622).

### Outcome Weighting — Inline Function (SR-06 Resolution)

The `infer_gate_result()` function in `tools.rs` takes `(outcome: Option<&str>,
pass_count: u32)` and returns `GateResult`. The crt-050 weighting step needs
only a simpler two-valued mapping: "pass" → `1.0`, "rework"/"fail" → `0.5`.

**Decision (SR-06):** Do NOT call `infer_gate_result()` from the rebuild path.
Instead, inline a minimal `outcome_weight(outcome: &str) -> f32` free function
private to `phase_freq_table.rs`. Rationale:

1. `infer_gate_result` requires `pass_count: u32` (a stateful parameter computed
   from cross-session scan) that has no meaning in the per-row weighting context.
2. Importing from `tools.rs` would create an intra-crate dependency from
   `services/` into `mcp/` — an inappropriate layering violation.
3. The weighting logic (two-valued: pass=1.0, else=0.5) is simpler than
   `infer_gate_result`'s four-valued classification — extracting the full
   function would overengineer the crt-050 use case.
4. If outcome vocabulary drift becomes a concern, both sites should be unified
   at that time under a shared `outcome_vocab` module. That refactor is deferred.

`outcome_weight(outcome: &str) -> f32` implements:
- `outcome.to_lowercase().contains("pass")` → `1.0`
- `outcome.to_lowercase().contains("rework") || contains("fail")` → `0.5`
- All other strings (including "unknown", empty) → `1.0` (degrade gracefully,
  AC-05 contract: missing outcome = unweighted = weight 1.0)

## Component Interactions

```
background.rs (run_single_tick)
  └─► PhaseFreqTable::rebuild(store, phase_freq_lookback_days)
        ├─► store.query_phase_freq_observations(lookback_days)   [Query A]
        │     SQL: observations JOIN entries → Vec<PhaseFreqRow> (raw freq)
        ├─► store.query_phase_outcome_map()                      [Query B]
        │     SQL: cycle_events JOIN sessions → Vec<PhaseOutcomeRow>
        ├─► apply_outcome_weights(rows_a, rows_b)                [Rust post-process]
        │     Build HashMap<(phase, feature_cycle), f32> from rows_b
        │     Multiply each PhaseFreqRow.freq by weight (default 1.0)
        │     Output: Vec<PhaseFreqRow> with weighted freq values
        └─► [rank normalization — unchanged]
              Group by (phase, category), rank within bucket, store (entry_id, score)

PhaseFreqTable (in memory)
  ├─► phase_affinity_score(entry_id, category, phase)   [search hot path, unchanged]
  └─► phase_category_weights()                           [W3-1 cold-start, off hot path]

status.rs (run_single_tick, step 4)
  ├─► warn_phase_freq_lookback_mismatch(...)             [renamed from crt-036]
  └─► warn_observations_coverage(...)                    [new, AC-11]
```

## Technology Decisions

See individual ADRs for rationale. Summary:

| Decision | Choice | ADR | Unimatrix ID |
|----------|--------|-----|--------------|
| Two-query vs. single SQL | Two queries + Rust post-process | ADR-001 | #4223 |
| Query A SQL structure | `observations JOIN entries` with `json_extract` + CAST | ADR-002 | #4224 |
| Outcome weighting function | Inline `outcome_weight()` in `phase_freq_table.rs` | ADR-003 | #4225 |
| Config field rename | `phase_freq_lookback_days` with serde alias | ADR-004 | #4226 |
| SR-01 storage contract | No double-encoding on hook path; pure-SQL valid | ADR-005 | #4227 |
| ts_millis unit contract | `MILLIS_PER_DAY` constant + pre-computed Rust cutoff | ADR-006 | #4228 |
| DB column name for hook filter | `o.hook` (not `o.hook_event`) | ADR-007 | #4229 |
| `phase_category_weights()` formula | Normalized bucket size (fraction of phase reads) | ADR-008 | #4230 |

## Integration Points

### Existing interfaces consumed unchanged

- `PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>` — shared handle
- `PhaseFreqTable::phase_affinity_score()` — search hot path (no change)
- `PhaseFreqTable::new()`, `new_handle()` — cold-start constructors (no change)
- `PhaseFreqRow { phase, category, entry_id, freq }` — row type reused as-is
- `unimatrix_store::PhaseFreqRow` re-export from crate root — unchanged

### New interfaces introduced

- `SqlxStore::query_phase_freq_observations(lookback_days: u32) -> Result<Vec<PhaseFreqRow>>`
  — replaces `query_phase_freq_table`
- `SqlxStore::query_phase_outcome_map() -> Result<Vec<PhaseOutcomeRow>>`
  — new; consumed only by `PhaseFreqTable::rebuild()`
- `PhaseOutcomeRow { phase: String, feature_cycle: String, outcome: String }`
  — new struct in `unimatrix-store`; not re-exported (internal to rebuild)
- `PhaseFreqTable::phase_category_weights(&self) -> HashMap<(String, String), f32>`
  — new public method; off the search hot path
- `InferenceConfig::phase_freq_lookback_days: u32` — renamed field
- `InferenceConfig::min_phase_session_pairs: u32` — new field (AC-11 threshold,
  default 5, validated [1, 1000])

### Deleted interfaces

- `SqlxStore::query_phase_freq_table(lookback_days: u32)` — one call site, deleted

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `query_phase_freq_observations` | `async fn(lookback_days: u32) -> Result<Vec<PhaseFreqRow>>` | `unimatrix-store/src/query_log.rs` (or `phase_freq.rs`) |
| `query_phase_outcome_map` | `async fn() -> Result<Vec<PhaseOutcomeRow>>` | `unimatrix-store/src/query_log.rs` (or `phase_freq.rs`) |
| `PhaseOutcomeRow` | `struct { phase: String, feature_cycle: String, outcome: String }` | `unimatrix-store` (internal, not re-exported) |
| `phase_category_weights` | `pub fn(&self) -> HashMap<(String, String), f32>` | `unimatrix-server/src/services/phase_freq_table.rs` |
| `outcome_weight` | `fn(outcome: &str) -> f32` — private, returns 1.0/0.5/1.0 | `unimatrix-server/src/services/phase_freq_table.rs` |
| `phase_freq_lookback_days` | `pub u32` on `InferenceConfig`, `#[serde(alias = "query_log_lookback_days")]` | `unimatrix-server/src/infra/config.rs` |
| `min_phase_session_pairs` | `pub u32` on `InferenceConfig`, default 5, range [1, 1000] | `unimatrix-server/src/infra/config.rs` |
| DB column: `observations.hook` | `TEXT NOT NULL` — filter value is `'PreToolUse'` | `unimatrix-store/src/db.rs` line 818 |
| DB column: `observations.ts_millis` | `INTEGER NOT NULL` — millisecond epoch | `unimatrix-store/src/db.rs` line 817 |

## Data Flow

```
observations table
  columns: session_id, ts_millis, hook, tool, input, phase
  relevant rows: hook = 'PreToolUse',
                 tool IN ('context_get', 'mcp__unimatrix__context_get',
                          'context_lookup', 'mcp__unimatrix__context_lookup'),
                 json_extract(input, '$.id') IS NOT NULL,
                 phase IS NOT NULL,
                 ts_millis > (strftime('%s', 'now') - ?1 * 86400) * 1000
  ↓ JOIN entries ON CAST(json_extract(input, '$.id') AS INTEGER) = entries.id
  ↓ GROUP BY phase, category, entry_id
  ↓ ORDER BY phase, category, COUNT(*) DESC
→ Vec<PhaseFreqRow> [raw freq counts]

cycle_events JOIN sessions
  filter: event_type = 'cycle_phase_end', phase IS NOT NULL, outcome IS NOT NULL
→ Vec<PhaseOutcomeRow> [(phase, feature_cycle, outcome)]

Rust post-process:
  Build HashMap<(phase, feature_cycle), f32>:
    outcome_weight(outcome) → weight per (phase, cycle)
  For each PhaseFreqRow:
    Look up weight by (row.phase, session_cycle_id_for_row) → default 1.0
    Multiply row.freq by weight → weighted freq (f32, stored as i64 via cast)

NOTE: The observations table does not carry feature_cycle directly. The
outcome weighting applies at the phase level across all cycles — the weight map
key is (phase, feature_cycle), but the observations rows only carry phase.
The correct implementation joins observations → sessions → cycle_events to
get the per-session feature_cycle for each observation row. See ADR-001 for the
chosen approach: Query B fetches all (phase, feature_cycle, outcome) pairs;
the Rust post-process aggregates weights per-phase (averaging or taking the
best available weight across all cycles for that phase). See ADR-001 for
the per-phase weight aggregation strategy.

↓ apply_outcome_weights(rows_a, rows_b) → Vec<PhaseFreqRow> with weighted freq
↓ group by (phase, category)
↓ rank-normalize within each bucket (unchanged col-031 ADR-001 formula)
→ PhaseFreqTable { table: HashMap<(String,String), Vec<(u64,f32)>>, use_fallback: false }

phase_category_weights():
  For each (phase, category) bucket in table:
    total_entries_for_phase = sum of bucket sizes across all categories for that phase
    weight = bucket.len() as f32 / total_entries_for_phase as f32
  Returns HashMap<(String, String), f32> summing to 1.0 per phase
  Returns empty map when use_fallback = true
```

## Observations Coverage Diagnostic (AC-11)

The crt-036 diagnostic at `status.rs` checks `phase_freq_lookback_days` against
the K-cycle retention window. That diagnostic is updated to reference the new
field name and to note that it governs the `observations` window (not `query_log`).

**Important:** The crt-036 diagnostic logic (comparing `phase_freq_lookback_days`
to the oldest retained cycle's `computed_at`) remains semantically valid for
the `observations` source because: (a) observations are linked to sessions which
are linked to feature cycles, (b) observations within the lookback window that
belong to reviews pruned by K-cycle GC will not match any live cycle_events
outcome rows — they are still counted as freq but weighted 1.0 (graceful
degradation). The diagnostic continues to fire as an advisory warning.

The new `warn_observations_coverage()` function adds a parallel check:
- Count distinct `(phase, session_id)` pairs in `observations` within the
  lookback window that have `phase IS NOT NULL` and matching tool names.
- If count < `InferenceConfig::min_phase_session_pairs` (default 5), emit
  `tracing::warn!` advising sparse signal.
- This count is computed via a SQL scalar subquery in `status.rs` (or a new
  dedicated store fn), not inside `rebuild()`. The rebuild returns its result
  regardless of coverage.

## Open Questions

**OQ-1 (per-phase weight aggregation for Query B) — DECIDED:** Use mean-weight
aggregation. When Query B returns multiple `(phase, feature_cycle)` rows, compute
the mean outcome weight across all cycles for each phase. Best-weight rewards phases
that ever passed even when pass is the exception; mean-weight is proportional to
actual pass rate and produces a more honest signal. This decision is recorded in
ADR-001 and restated in IMPLEMENTATION-BRIEF constraint #6. RISK-TEST-STRATEGY R-03
provides the concrete test scenarios validating correct aggregation behavior. No
discretion is left to the implementer.

**OQ-2 (PhaseOutcomeRow visibility):** `PhaseOutcomeRow` is only consumed by
`PhaseFreqTable::rebuild()`. It should remain pub(crate) or internal to
`unimatrix-store`. If it ever needs cross-crate access, re-evaluate at that time.

**OQ-3 (min_phase_session_pairs default):** The default value of 5 is a
pragmatic minimum. The actual meaningful threshold is workload-dependent.
This is advisory-only (emit warning, not block rebuild), so the risk of a wrong
default is low.
