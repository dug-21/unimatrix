# SPECIFICATION: crt-050 — Phase-Conditioned Category Affinity (Explicit Read Rebuild)

GH Issue: #542

---

## Objective

Replace the `PhaseFreqTable` rebuild source from `query_log` search exposures (surfaced-but-not-read signal) with explicit-read observations from the `observations` table (deliberately-fetched signal), using `context_get` and single-ID `context_lookup` PreToolUse rows joined to `entries.category`. Apply outcome-based weighting from `cycle_events` to discount phase reads from cycles that required rework or failed. Expose the resulting per-`(phase, category)` weight map as a stable accessor for W3-1 GNN cold-start initialization, replacing the hand-tuned WA-2 constants.

---

## Ubiquitous Language

| Term | Definition |
|------|------------|
| **Explicit read** | A `context_get` or single-ID `context_lookup` PreToolUse event recorded in `observations`. Represents a deliberate agent fetch, distinct from a search exposure. |
| **Search exposure** | An entry appearing in a `context_search` result set. Recorded in `query_log`. NOT an explicit read. |
| **PhaseFreqTable** | In-memory tick-rebuild cache keyed by `(phase, category)` → `Vec<(entry_id, rank_score)>`. Rebuilt each background tick. |
| **PhaseFreqTableHandle** | `Arc<RwLock<PhaseFreqTable>>` — thread-safe shared reference; background tick is sole writer. |
| **PhaseFreqRow** | `(phase: String, category: String, entry_id: u64, freq: u32)` — the row shape returned by the store query and consumed by `PhaseFreqTable::rebuild()`. `freq` is now outcome-weighted explicit read count, not raw exposure count. |
| **OutcomeWeightMap** | `HashMap<(phase: String, feature_cycle: String), f32>` — built from `cycle_events.cycle_phase_end` rows during rebuild. Maps each `(phase, cycle)` pair to a weight: 1.0 (pass), 0.5 (rework or fail), or 1.0 (no row / default). |
| **phase_category_weights** | `HashMap<(String, String), f32>` keyed by `(phase, category)`. Each value is the fraction of total outcome-weighted explicit reads for that phase attributable to that category (probability distribution: sums to 1.0 per phase). Returns empty map when `use_fallback = true`. |
| **cold-start** | State where `PhaseFreqTable.use_fallback = true` and `table` is empty. Occurs until first successful rebuild with at least one observation row. |
| **retain-on-error** | Behavior where a failed tick rebuild leaves the previous `PhaseFreqTable` contents unchanged rather than replacing with empty. |
| **rank-based normalization** | The scoring formula from col-031 ADR-001: `score = 1.0 - ((rank - 1) as f32 / N as f32)`, where rank is 1-indexed by descending `freq`. Operates on ordering, not absolute freq values. |
| **storage contract** | Confirmed guarantee (ADR-005): `observations.input` is stored as a JSON object on all write paths. `serde_json::to_string(Value::Object{...})` on the hook-listener path produces `'{"id":42}'`, not a double-encoded string. `json_extract(input, '$.id')` works for all rows. |
| **observations coverage count** | Distinct `(phase, session_id)` pair count within the lookback window. Used as signal-quality floor. |
| **minimum coverage threshold** | Configurable minimum observations coverage count below which `PhaseFreqTable` degrades to `use_fallback = true` and emits a tick-time warning. |
| **ts_millis** | Millisecond-epoch timestamp column in `observations`. Contrast with `query_log.ts` which is second-epoch. |
| **W3-1 GNN cold-start** | The future GNN initialization path (ASS-029) that will consume `phase_category_weights()` to replace hand-tuned WA-2 constants. |

---

## Functional Requirements

### FR-01: Rebuild source migration
`PhaseFreqTable::rebuild()` must source `(phase, category, entry_id, freq)` aggregates from the `observations` table, not `query_log`. The old store function `query_phase_freq_table` must be deleted.

*Verification: unit test (no `query_log` read path in rebuild); code review.*

### FR-02: Tool name filter — base aggregate query
The base aggregate SQL must filter `observations.tool` to exactly the 4-entry IN clause: `('context_get', 'mcp__unimatrix__context_get', 'context_lookup', 'mcp__unimatrix__context_lookup')`. No `REPLACE`/`SUBSTR` normalization in SQL — the 4-entry enumeration is the correct and indexable approach (confirmed by crt-049 AC-06: the `mcp__unimatrix__` prefix is the only hook-listener prefix variant written).

*Verification: unit test asserting prefixed tool names produce rows; unit test asserting other tool names are excluded.*

### FR-03: Single-ID predicate — base aggregate query
The base aggregate SQL must include `json_extract(o.input, '$.id') IS NOT NULL` as the filter to exclude filter-based `context_lookup` calls (which carry no `$.id` field). This distinguishes single-entry explicit reads from multi-filter lookups.

*Verification: unit test asserting filter-based lookup observations (no `$.id`) produce zero rows.*

### FR-04: PreToolUse-only filter
The base aggregate SQL must include `o.hook_event = 'PreToolUse'` (or the equivalent column name in the `observations` schema). PostToolUse rows for `context_get`/`context_lookup` must not be counted; without this filter, each explicit read is double-counted.

*Verification: unit test with paired Pre/Post observations asserting only one count.*

### FR-05: CAST mandatory in JOIN predicate
`CAST(json_extract(o.input, '$.id') AS INTEGER)` must be used in the `JOIN entries e ON ...` predicate. Omitting CAST causes a silent text-to-integer type mismatch returning zero rows (col-031 R-05). CAST correctly handles both integer-form IDs (`{"id": 42}`) and string-form IDs (`{"id": "42"}`).

*Verification: unit test with string-form ID observation asserting the row is returned.*

### FR-06: Millisecond-epoch lookback boundary
The time window filter must use `o.ts_millis > (strftime('%s', 'now') - ?1 * 86400) * 1000`. The `* 1000` factor is mandatory; `query_log.ts` is second-epoch but `observations.ts_millis` is millisecond-epoch. A missing factor produces a 1000x window error (too wide or too narrow) with no error logged.

*Verification: unit test inserting observations at boundary ± 1ms asserting correct inclusion/exclusion.*

### FR-07: Storage contract confirmed — pure-SQL approach valid
The storage contract for `observations.input` is confirmed by the architect (ADR-005): the hook-listener write path calls `serde_json::to_string(Value::Object{...})`, producing a plain JSON object string (e.g., `'{"id":42}'`) — not a double-encoded string. `json_extract(o.input, '$.id')` works correctly for all write paths. The pure-SQL extraction approach in Query A is valid for all rows (direct-MCP and hook-path). No two-phase extraction is required.

*Verification: unit test inserting hook-path-form observations (JSON object input) and asserting non-zero rebuild result.*

### FR-08: Outcome weighting — two-query approach
Outcome weighting must be implemented as a two-query Rust post-process, not a single SQL join. Query B fetches `(phase, feature_cycle, outcome)` from `cycle_events` joined to `sessions`. The Rust post-process builds an `OutcomeWeightMap` using `infer_gate_result()` logic: case-insensitive substring match — "pass" → 1.0, "rework" → 0.5, "fail" → 0.5. Unmatched / no-row → 1.0.

*Verification: unit tests (c) and (d) in AC-13.*

### FR-09: outcome-to-weight mapping
The outcome weight mapping must be: "pass" (case-insensitive contains) → 1.0; "rework" (case-insensitive contains) → 0.5; "fail" (case-insensitive contains) → 0.5. When a `(phase, feature_cycle)` pair has no `cycle_phase_end` row, or when `sessions.feature_cycle` is NULL (sessions predating col-022), the default weight is 1.0. This default must not error and must not escalate `use_fallback`.

*Verification: unit test (b), (c), (d) in AC-13; SR-05 degradation test.*

### FR-10: NULL feature_cycle degradation
When `sessions.feature_cycle` is NULL (pre-col-022 sessions), the `cycle_events → sessions` join for outcome weighting produces zero rows for those sessions. The rebuild must treat all observations from those sessions as weight 1.0 (unweighted), not as an error. This is a silent, correct degradation — not a fallback escalation.

*Verification: integration test with pre-col-022-style session (NULL feature_cycle) asserting rebuild completes and returns non-empty rows with weight 1.0.*

### FR-11: Cold-start and retain-on-error contracts preserved
All four existing `PhaseFreqTable` integration contracts must be preserved:
1. Empty rebuild result → `use_fallback = true` (cold-start behavior).
2. `phase_affinity_score()` returns 1.0 for cold-start / absent phase / absent entry (col-031 ADR-003).
3. Retain-on-error: a tick rebuild error leaves the previous table contents unchanged.
4. Poison recovery: all `RwLock` acquisitions use `.unwrap_or_else(|e| e.into_inner())`.

*Verification: unit test (a) in AC-13; existing contract tests must continue passing.*

### FR-12: phase_category_weights() accessor
`PhaseFreqTable` must expose a `pub fn phase_category_weights(&self) -> HashMap<(String, String), f32>` method. For each `(phase, category)` bucket, the weight is the fraction of total outcome-weighted explicit reads for that phase attributable to that category (normalized bucket size = probability distribution summing to 1.0 per phase). Returns an empty map when `use_fallback = true`. This method is not on the search hot path — it is called only at GNN initialization time.

*Verification: unit test (g) in AC-13.*

### FR-13: Delete old query_phase_freq_table
The old `query_phase_freq_table` store function must be deleted. It has exactly one call site (the one being replaced). Deleting it eliminates misleading ambiguity about the active rebuild path. All existing tests for the old function must be rewritten for the new signal source.

*Verification: grep confirms no remaining call sites; compilation succeeds.*

### FR-14: Rename query_log_lookback_days with serde alias
`InferenceConfig::query_log_lookback_days` must be renamed to `phase_freq_lookback_days` with `#[serde(alias = "query_log_lookback_days")]`. The old name is semantically incorrect once `query_log` is not the signal source. The serde alias preserves backward compatibility for existing TOML configs. All test fixtures that construct `InferenceConfig` as a struct literal must be updated (SR-04: these are compile errors, not silent failures).

*Verification: grep for `query_log_lookback_days` field usage in struct literals; compilation succeeds.*

### FR-15: Update crt-036 tick-time diagnostic
The crt-036 ADR-003 tick-time diagnostic warning (entry #3917) must be updated to reference `phase_freq_lookback_days` and note it governs the `observations` window, not `query_log`. The check logic may require revision: the original diagnostic compared lookback against oldest retained cycle's `computed_at`; this heuristic may no longer apply if `observations` are not pruned by the same K-cycle retention logic. The architect must evaluate and document the revised diagnostic scope.

*Verification: updated warning text in tick log output; code review.*

### FR-16: Observations-coverage diagnostic
A new tick-time diagnostic must emit a `tracing::warn!` when the distinct `(phase, session_id)` observation pair count within the lookback window falls below the minimum coverage threshold (see NFR-04). This warning is advisory, not a gate — the rebuild proceeds. The threshold must be configurable (see FR-17).

*Verification: unit test asserting warning is emitted below threshold; unit test asserting no warning above threshold.*

### FR-17: Minimum coverage threshold configuration
A configurable minimum `(phase, session_id)` pair count must be added to `InferenceConfig` as `min_phase_session_pairs: u32` (default 5, range [1, 1000]). When the distinct pair count from the base aggregate query falls below this threshold, `PhaseFreqTable` must degrade to `use_fallback = true` and emit the coverage diagnostic warning. This prevents misleading sparse-data phase weights from feeding the scoring pipeline.

*Verification: unit test asserting `use_fallback = true` when pair count is below threshold; unit test asserting normal operation above threshold.*

---

## Non-Functional Requirements

### NFR-01: Tick latency — no regression
`PhaseFreqTable::rebuild()` runs inside the existing `run_single_tick` timeout. The two-query approach must not increase average tick duration beyond the existing `query_log`-based rebuild. Query A is a SQL aggregate (not a full table scan of raw rows); Query B fetches only `cycle_phase_end` rows (sparse). Total rebuild wall time must be measurable and logged at `tracing::debug!` level.

*Measurement: tick log timing in staging; cargo bench if a micro-benchmark exists.*

### NFR-02: MRR gate — no regression
The eval harness regression gate must pass: MRR must not decrease below 0.2788 (the post-PPR-expander baseline). The 1,761 scenarios in `product/research/ass-039/harness/scenarios.jsonl` are the canonical measurement instrument. A regression blocks merge.

*Measurement: eval harness run against scenarios.jsonl; MRR ≥ 0.2788 required.*

### NFR-03: Correctness of weighted freq ordering
The rank-based normalization formula (col-031 ADR-001) operates on the ordering of `freq` values within a `(phase, category)` bucket, not their absolute magnitudes. Outcome weighting scales `freq` values — this scaling must preserve relative ordering within a bucket. The formula is invariant to affine scaling of `freq` within a bucket when all rows in the bucket share the same weight, but is not invariant when rows within the same bucket come from different-weight cycles. The Rust post-process must accumulate weighted counts correctly before rank ordering.

*Measurement: unit test with mixed-weight observations asserting correct rank order.*

### NFR-04: Minimum coverage threshold default
The minimum coverage threshold (FR-17) is `min_phase_session_pairs = 5` (architect decision, ADR-007 / ARCHITECTURE.md). The value is intentionally conservative — low enough to not trigger spuriously in development/test environments while providing a non-zero signal-quality floor. Range [1, 1000] validated on deserialization.

*Measurement: unit test with threshold = N asserting correct behavior at N-1 and N+1 pairs.*

### NFR-05: No schema migration required
This feature must not add new tables or columns. All required columns (`observations.phase`, `observations.input`, `observations.hook_event`, `observations.ts_millis`, `observations.tool`, `observations.session_id`, `entries.category`, `cycle_events.event_type`, `cycle_events.phase`, `cycle_events.outcome`, `sessions.feature_cycle`) already exist in the merged codebase (crt-043, crt-049).

*Measurement: confirm no `ALTER TABLE` or `CREATE TABLE` statements in the implementation.*

### NFR-06: No change to scoring callers
`phase_affinity_score()` method signature and return semantics must be unchanged. `PhaseFreqTableHandle` type alias must be unchanged. Fused scoring and PPR callers must require no modification.

*Measurement: grep confirms no changes to calling sites; compilation without changes to callers.*

### NFR-07: phase_category_weights() not on hot path
`phase_category_weights()` must not be called on the search hot path. It is for GNN initialization only. The implementation must not acquire any lock or perform any computation for this method during search scoring.

*Measurement: code review; no call to `phase_category_weights()` in `search.rs` pipeline.*

---

## Acceptance Criteria

### AC-01: Rebuild source is observations, not query_log
**Verification**: Unit test. Populate `observations` with explicit read rows and empty `query_log`; call `rebuild()`. Assert non-empty table and `use_fallback = false`. Populate only `query_log` and empty `observations`; call `rebuild()`. Assert `use_fallback = true`.

### AC-02: Tool name filter covers bare and prefixed variants
**Verification**: Unit test. Insert observations with all four tool names (`context_get`, `mcp__unimatrix__context_get`, `context_lookup`, `mcp__unimatrix__context_lookup`); assert all four produce rows. Insert observation with `context_search`; assert it produces zero rows.

### AC-03: CAST used in JOIN predicate; string-form IDs handled
**Verification**: Unit test. Insert observation with `{"id": "42"}` (string-form ID); assert the row is returned and matched to entry 42. This confirms CAST handles string-form IDs and prevents text-to-integer JOIN mismatch.

### AC-04: Outcome weighting via two-query Rust post-process
**Verification**: Unit test. Populate Query A rows and Query B rows with known outcomes; assert weighted `freq` values are applied correctly before rank ordering. Confirm `infer_gate_result()`-equivalent logic is used (not a reimplemented CASE expression in SQL).

### AC-05: Graceful degradation when no cycle_phase_end history
**Verification**: Unit test. Rebuild with observations present but no `cycle_phase_end` rows. Assert `use_fallback = false`, no error, all rows weighted 1.0.

### AC-SV-01: Storage contract confirmed (SR-01 resolved)
**Verification**: ADR-005 documents the confirmed storage contract. The hook-listener write path produces a JSON object string — `json_extract(input, '$.id')` works for all rows. The pure-SQL approach is valid; no two-phase extraction is required. This criterion is satisfied by ADR-005 existing in Unimatrix (#4227). No blocking implementation gate.

### AC-06: All existing PhaseFreqTable contracts preserved
**Verification**: Existing contract tests pass without modification. Specifically:
- `use_fallback = true` on empty rebuild result.
- `phase_affinity_score()` returns 1.0 for absent phase/entry.
- Retain-on-error: previous table retained on tick rebuild error.
- Poison recovery: `.unwrap_or_else(|e| e.into_inner())` on all lock acquisitions.

### AC-07: ts_millis lookback boundary correct
**Verification**: Unit test. Insert two observations — one at `now - lookback + 500ms` (inside window) and one at `now - lookback - 500ms` (outside window). Assert only the inside-window observation contributes to the rebuild.

### AC-08: phase_category_weights() returns correct distribution
**Verification**: Unit tests (g): empty map when `use_fallback = true`; non-empty map with correct per-phase probability distribution (values sum to 1.0 per phase) when table is populated.

### AC-09: Old query_phase_freq_table deleted
**Verification**: Code review + grep. No reference to `query_phase_freq_table` in the codebase after implementation. All tests previously covering this function are rewritten for the new query.

### AC-10: phase_freq_lookback_days rename with serde alias
**Verification**: Unit test asserting `InferenceConfig` deserialized from `{"query_log_lookback_days": 30}` produces `phase_freq_lookback_days = 30` (alias works). Struct-literal usages in tests compile without the old field name.

### AC-11: Updated diagnostic references phase_freq_lookback_days; coverage diagnostic added
**Verification**: Code review. Tick-time warning references `phase_freq_lookback_days`, not `query_log_lookback_days`. A new observations-coverage diagnostic emits `tracing::warn!` when distinct `(phase, session_id)` count falls below threshold.

### AC-12: MRR eval harness gate passes
**Verification**: Eval harness. Run scenarios.jsonl (1,761 scenarios) against the implementation. MRR must be ≥ 0.2788. This is a hard gate — merge blocked on failure.

### AC-13: Unit test coverage for specified scenarios
**Verification**: Each sub-item is a distinct test:
- (a) Empty observations → `use_fallback = true`.
- (b) Pass-outcome rows weighted 1.0.
- (c) Rework-outcome rows weighted 0.5.
- (d) Fail-outcome rows weighted 0.5.
- (e) Missing outcome degrades to unweighted (weight 1.0), no error.
- (f) Prefixed tool names (`mcp__unimatrix__context_get`, `mcp__unimatrix__context_lookup`) included.
- (g) Filter-based `context_lookup` (no `$.id` in input) excluded.
- (h) `phase_category_weights()` returns empty map on cold-start; non-empty map on populated table with correct probability distribution.

### AC-14: Minimum coverage threshold gate
**Verification**: Unit test. Set minimum threshold to N. Insert N-1 distinct `(phase, session_id)` pairs; assert `use_fallback = true` and coverage warning emitted. Insert N distinct pairs; assert `use_fallback = false` (assuming non-empty observations).

### AC-15: NULL feature_cycle degradation
**Verification**: Integration test. Create sessions with `feature_cycle = NULL`. Insert observations attributed to those sessions. Call `rebuild()`. Assert non-empty table (not errored), all rows weighted 1.0, `use_fallback = false` if pair count meets threshold.

---

## Domain Models

### PhaseFreqTable (unchanged internal structure)

```
PhaseFreqTable {
    table: HashMap<(phase: String, category: String), Vec<(entry_id: u64, rank_score: f32)>>,
    use_fallback: bool,
}
```

`rank_score` = `1.0 - ((rank - 1) as f32 / N as f32)` where rank is 1-indexed by descending weighted freq, N = bucket size. Formula operates on ordering only — invariant to uniform affine scaling within a bucket.

### PhaseFreqRow (unchanged, reused for new query)

```
PhaseFreqRow {
    phase: String,
    category: String,
    entry_id: u64,
    freq: u32,  // now outcome-weighted explicit read count, not raw exposure count
}
```

Declared in `unimatrix-store/src/query_log.rs`, re-exported from crate root. No type signature change.

### OutcomeWeightMap (new, ephemeral — not persisted)

```
OutcomeWeightMap = HashMap<(phase: String, feature_cycle: String), f32>
```

Built during each rebuild from Query B results. Weight values: 1.0 (pass or no row), 0.5 (rework or fail). Keyed by `(phase, feature_cycle)` to allow per-cycle per-phase weighting. Discarded after weighted freq accumulation.

### phase_category_weights return type

```
HashMap<(phase: String, category: String), f32>
```

Each value = `bucket_weighted_freq_sum / total_weighted_freq_for_phase`. Sums to 1.0 per phase. Empty map when `use_fallback = true`. Computed on demand from `table` contents; not stored as a field.

### InferenceConfig (modified field)

```
InferenceConfig {
    // Renamed from query_log_lookback_days
    phase_freq_lookback_days: u32,  // #[serde(alias = "query_log_lookback_days")]

    // New field
    min_phase_session_pairs: u32,  // minimum distinct (phase, session) pairs
    // ...other fields unchanged
}
```

---

## User Workflows

### Workflow 1: Background tick rebuild (normal path)

1. `run_single_tick` fires on schedule.
2. `PhaseFreqTable::rebuild()` is called.
3. Query A executes: SQL aggregate over `observations` joined to `entries` — returns `Vec<PhaseFreqRow>` with raw explicit-read counts.
4. Query B executes: `cycle_events` joined to `sessions` — returns `Vec<(phase, feature_cycle, outcome)>`.
5. Rust post-process builds `OutcomeWeightMap` from Query B results using `infer_gate_result()` substring matching.
6. Rust post-process applies weights to Query A rows: for each row, multiply `freq` by the matching `OutcomeWeightMap` entry (or 1.0 if absent).
7. Distinct `(phase, session_id)` count is computed from the base observations; if below `min_phase_session_pairs`, set `use_fallback = true` and emit coverage warning. Rebuild ends early.
8. Rows are grouped and rank-normalized into `table` per the col-031 ADR-001 formula.
9. `PhaseFreqTableHandle` write lock is acquired; table is replaced with new contents, `use_fallback = false`.
10. Lock is released; next search query uses the updated table.

### Workflow 2: Background tick rebuild (no observations)

1. `PhaseFreqTable::rebuild()` is called.
2. Query A returns empty `Vec`.
3. `use_fallback = true` is set; previous table is cleared (cold-start semantics).
4. Callers receive neutral scores until next successful tick.

### Workflow 3: Background tick rebuild (error path)

1. `PhaseFreqTable::rebuild()` encounters a store error (DB unavailable, etc.).
2. Error is logged at `tracing::error!`.
3. Previous `PhaseFreqTable` contents are retained unchanged (retain-on-error).
4. `use_fallback` state is not changed.

### Workflow 4: Search hot path (unchanged)

1. `SearchService` scoring acquires a read lock on `PhaseFreqTableHandle`.
2. Fused scoring checks `use_fallback`: if true, sets `phase_explicit_norm = 0.0`.
3. If false, calls `phase_affinity_score(phase, entry_id)` — returns rank score or 1.0 (neutral) for absent entries.
4. Lock released immediately after score extraction.

### Workflow 5: W3-1 GNN cold-start initialization (future)

1. W3-1 GNN initialization calls `phase_category_weights()` on `PhaseFreqTable`.
2. Method computes per-phase probability distribution from `table` contents.
3. Returns `HashMap<(String, String), f32>` for GNN weight initialization.
4. If `use_fallback = true`, returns empty map; GNN uses alternative initialization.

---

## Constraints

### C-01: No schema migration
All required columns exist in the merged codebase. No `ALTER TABLE` or `CREATE TABLE` statements permitted.

### C-02: crt-049 storage contract (SR-01 — resolved, not a blocker)
`json_extract(o.input, '$.id')` requires `observations.input` to be a JSON object. ADR-005 confirms this is satisfied on all write paths — the hook-listener uses `serde_json::to_string(Value::Object{...})`, producing a plain JSON object string. The pure-SQL approach is valid. No two-phase extraction is required.

### C-03: CAST mandatory
`CAST(json_extract(input, '$.id') AS INTEGER)` is mandatory in the JOIN predicate. Omitting CAST causes a text-to-integer type mismatch returning zero rows silently (col-031 R-05).

### C-04: ts_millis × 1000
The lookback boundary formula must multiply the seconds-based cutoff by 1000. Omitting the factor produces a 1000x window error with no error logged.

### C-05: 4-entry IN clause for tool names
The IN clause must enumerate all four variants. No REPLACE/SUBSTR normalization in SQL. SQLite has no equivalent of `normalize_tool_name()`.

### C-06: hook_event = 'PreToolUse' required
Without this filter, PostToolUse rows double-count each explicit read.

### C-07: infer_gate_result() reuse for outcome matching
The outcome substring-match logic must reuse or be consistent with `infer_gate_result()` in `tools.rs` (col-026 R-03). Duplicate implementations of the same substring-match vocabulary are a drift risk for future outcome string vocabulary changes. The architect must resolve the module boundary (SR-06): either extract `infer_gate_result()` to a shared location, or inline equivalent logic with a comment referencing the canonical location.

### C-08: PhaseFreqRow type unchanged
The existing `PhaseFreqRow` type (`phase, category, entry_id, freq`) must be reused. The `freq` field now carries outcome-weighted read count, not raw exposure count — semantics change, shape does not.

### C-09: No changes to scoring callers
`phase_affinity_score()` method signature, `PhaseFreqTableHandle` type alias, fused scoring, and PPR callers must remain unchanged.

### C-10: phase_category_weights() visibility
`phase_category_weights()` must be `pub` on `PhaseFreqTable`. W3-1 access from outside the crate may require a visibility change at W3-1 implementation time; this is a tracked open item, not a blocking constraint for crt-050.

### C-11: InferenceConfig struct literal changes
The `query_log_lookback_days` → `phase_freq_lookback_days` rename will cause compile errors at all struct-literal construction sites (not just TOML deserialization). The architect must audit all test fixtures that construct `InferenceConfig` directly (SR-04).

---

## Dependencies

### Crate dependencies (no new crates required)
- `unimatrix-store`: `PhaseFreqRow`, `SqlxStore`, `StoreError` — all existing.
- `unimatrix-server/src/services/phase_freq_table.rs` — the primary file under change.
- `unimatrix-server/src/mcp/tools.rs` — `infer_gate_result()` (SR-06: module boundary decision required).
- `unimatrix-server/src/background.rs` — `run_single_tick` wiring (unchanged, for reference).
- `unimatrix-core` — `Store` trait (unchanged).

### Feature dependencies
- **crt-049 (#539)** — merged (confirmed: commits eaed9428, 5a6850db, 813c4801). Provides `observations.input` with explicit read IDs. The storage contract (C-02) is the outstanding dependency requiring architectural resolution.
- **crt-043** — merged. Provides `observations.phase` column.
- **col-022** — merged. Provides `sessions.feature_cycle` column; pre-col-022 sessions have NULL `feature_cycle` (degradation path FR-10).
- **col-026** — merged. Provides `infer_gate_result()` in `tools.rs`.
- **col-031** — merged. Establishes rank-based normalization formula (ADR-001), cold-start contracts (ADR-003), and `query_log_lookback_days` (ADR-002 #3686).
- **crt-036** — merged. Establishes tick-time diagnostic for lookback alignment (ADR-003 #3917); must be updated in this feature.

### External schema dependencies
Tables and columns required (all present, no migration):
- `observations`: `session_id`, `ts_millis`, `hook_event`, `tool`, `input`, `phase`
- `entries`: `id`, `category`
- `cycle_events`: `cycle_id`, `event_type`, `phase`, `outcome`
- `sessions`: `feature_cycle`

### Future consumer
- **ASS-029 / W3-1 GNN**: Will consume `phase_category_weights()`. Visibility of `PhaseFreqTable` from outside `unimatrix-server` is a deferred decision (C-10, SR-07).

---

## NOT In Scope

- **W3-1 GNN implementation** (ASS-029). Only the `phase_category_weights()` accessor is in scope.
- **Changing w_phase_explicit or w_phase_histogram default values**. Those are W3-1's domain.
- **Changing PhaseFreqTable internal structure** — `table` field type, rank-based scoring formula, or `phase_affinity_score()` signature.
- **Adding a new DB table or schema migration**.
- **Changing how context_get or context_lookup write observations**.
- **Changing explicit_read_by_category in FeatureKnowledgeReuse** with a phase dimension (crt-049 field is cycle-scoped by design).
- **Phase-stratified goal-cluster retrieval** (deferred in crt-046).
- **Removing query_log from the codebase**. Only `query_phase_freq_table` (one function) is deleted.
- **Changing query_log.ts to millisecond epoch**. `observations.ts_millis` is the new source; `query_log.ts` remains as-is for any surviving query_log consumers.
- **Hardening outcome string vocabulary**. Future enum normalization is deferred.

---

## Open Questions

### OQ-01: SR-01 — Storage contract — RESOLVED
ADR-005 confirms: no double-encoding on write path. Pure-SQL `json_extract(input, '$.id')` approach is valid for all write paths. No Option A or B required.

### OQ-02: SR-06 — infer_gate_result() module boundary
The architect must decide whether to extract `infer_gate_result()` to a shared module (e.g., `unimatrix-core` or a new `unimatrix-server/src/util/` location) or inline equivalent logic in the rebuild path with a canonical reference comment. Duplication without coordination creates drift risk if outcome vocabulary changes.

### OQ-03: Minimum coverage threshold default value — RESOLVED
Default is 5 distinct `(phase, session_id)` pairs (architect decision). Field name: `min_phase_session_pairs` on `InferenceConfig`. Advisory warning only; not a rebuild gate.

### OQ-04: crt-036 diagnostic applicability
The original crt-036 diagnostic compared `phase_freq_lookback_days` against oldest retained cycle `computed_at` (a proxy for K-cycle retention coverage). This check was meaningful for `query_log` because `query_log` is pruned by cycle retention. The `observations` table may not be pruned by the same K-cycle retention logic. The architect must confirm whether the crt-036 diagnostic remains meaningful for the new source, and if not, what replaces it (the observations-coverage diagnostic from FR-16 may be sufficient).

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — found entry #4222 (observations→PhaseFreqTable SQL: ms epoch, tool prefix, and CAST are all mandatory); entry #3917 (crt-036 ADR-003 lookback diagnostic); entry #3677 (phase_affinity_score cold-start neutral return); entry #3699 (col-031 cold-start guard flag pattern). These informed the constraint section and SR-01 treatment.
- SR-01 storage contract status confirmed by direct code inspection of `listener.rs` (lines 2693–2696) and `knowledge_reuse.rs` (lines 76–103) at merged crt-049 HEAD: `input` is double-encoded on hook path. AC-SV-01 added as a blocking architect gate.
