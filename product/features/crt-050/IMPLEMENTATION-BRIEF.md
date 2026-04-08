# crt-050 Implementation Brief
# Phase-Conditioned Category Affinity (Explicit Read Rebuild)

GH Issue: #542

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-050/SCOPE.md |
| Architecture | product/features/crt-050/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-050/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-050/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-050/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| store-queries | pseudocode/store-queries.md | test-plan/store-queries.md |
| phase-freq-table | pseudocode/phase-freq-table.md | test-plan/phase-freq-table.md |
| config | pseudocode/config.md | test-plan/config.md |
| status-diagnostics | pseudocode/status-diagnostics.md | test-plan/status-diagnostics.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Replace `PhaseFreqTable::rebuild()` to source its `(phase, category, entry_id, freq)` aggregates from deliberate agent reads recorded in `observations` (via `context_get` and single-ID `context_lookup` PreToolUse events) instead of the noisy `query_log` search-exposure signal. Apply outcome-based weighting from `cycle_events` so phases that required rework or failure contribute half-weight signal. Expose the resulting per-`(phase, category)` weight distribution as a stable accessor for future W3-1 GNN cold-start initialization, replacing hand-tuned WA-2 constants.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Two-query vs. single SQL join | Two queries + Rust post-process (Option B): Query A aggregates observations→entries, Query B fetches cycle_events→sessions outcomes; Rust applies per-phase mean weights | ADR-001 (Unimatrix #4223) | architecture/ADR-001-two-query-rebuild-strategy.md |
| Query A SQL structure | `observations JOIN entries` with `json_extract` + mandatory CAST; 4-entry IN clause; `o.hook = 'PreToolUse'`; pre-computed `cutoff_millis` binding | ADR-002 (Unimatrix #4224) | architecture/ADR-002-query-a-sql-structure.md |
| Outcome weight function placement | Inline private `outcome_weight(outcome: &str) -> f32` in `phase_freq_table.rs`; do NOT call `infer_gate_result()` (signature mismatch + layering violation) | ADR-003 (Unimatrix #4225) | architecture/ADR-003-outcome-weight-function-placement.md |
| Config field rename | `query_log_lookback_days` → `phase_freq_lookback_days` with `#[serde(alias = "query_log_lookback_days")]`; all struct-literal sites updated | ADR-004 (Unimatrix #4226) | architecture/ADR-004-config-field-rename.md |
| observations.input storage contract | No double-encoding on hook-listener write path; `serde_json::to_string(Value::Object{...})` produces plain JSON object string; pure-SQL `json_extract` valid for all rows | ADR-005 (Unimatrix #4227) | architecture/ADR-005-observations-input-storage-contract.md |
| ts_millis unit contract | `MILLIS_PER_DAY: i64 = 86_400 * 1_000` constant; pre-compute `cutoff_millis` in Rust and bind as `i64`; SQL predicate `o.ts_millis > ?2` | ADR-006 (Unimatrix #4228) | architecture/ADR-006-ts-millis-unit-contract.md |
| DB column name for PreToolUse filter | `o.hook = 'PreToolUse'` (not `o.hook_event`); SCOPE.md draft SQL was wrong | ADR-007 (Unimatrix #4229) | architecture/ADR-007-hook-column-name.md |
| phase_category_weights() formula | Normalized bucket size: `bucket.len() / total_entries_for_phase` — probability distribution (breadth-based, sums to 1.0 per phase); empty map when `use_fallback = true` | ADR-008 (Unimatrix #4230) | architecture/ADR-008-phase-category-weights-formula.md |

---

## Files to Create / Modify

### Modify

| File | Change Summary |
|------|----------------|
| `crates/unimatrix-store/src/query_log.rs` | Delete `query_phase_freq_table`; add `query_phase_freq_observations` (Query A) and `query_phase_outcome_map` (Query B); add `PhaseOutcomeRow` struct; add `MILLIS_PER_DAY` constant |
| `crates/unimatrix-server/src/services/phase_freq_table.rs` | Replace `store.query_phase_freq_table()` call with two-query path + `apply_outcome_weights` Rust post-process; add `outcome_weight()` private fn; add `phase_category_weights()` public method |
| `crates/unimatrix-server/src/infra/config.rs` | Rename `query_log_lookback_days` → `phase_freq_lookback_days` with serde alias; add `min_phase_session_pairs: u32` (default 5, range [1,1000]); rename default fn; update validation error strings |
| `crates/unimatrix-server/src/services/status.rs` | Update `warn_phase_freq_lookback_mismatch` fn and field references; add `warn_observations_coverage()` fn (emits `tracing::warn!` when distinct `(phase, session_id)` count < threshold) |
| `crates/unimatrix-server/src/background.rs` | Update `inference_config.query_log_lookback_days` → `inference_config.phase_freq_lookback_days` (single site, confirmed line 622) |

### Possibly Create

| File | Change Summary |
|------|----------------|
| `crates/unimatrix-store/src/phase_freq.rs` | Optional: if the implementer moves the new query functions out of `query_log.rs` into a dedicated module (implementation-time discretion; `PhaseFreqRow` re-export must remain stable regardless) |

---

## Data Structures

### PhaseFreqRow (unchanged — reused for new query)
```rust
// Declared in unimatrix-store/src/query_log.rs, re-exported from crate root
pub struct PhaseFreqRow {
    pub phase: String,
    pub category: String,
    pub entry_id: u64,
    pub freq: u32,  // semantics change: now outcome-weighted explicit-read count; shape unchanged
}
```

### PhaseOutcomeRow (new — internal to store, not re-exported)
```rust
// Declared in unimatrix-store (query_log.rs or phase_freq.rs)
struct PhaseOutcomeRow {
    phase: String,
    feature_cycle: String,
    outcome: String,
}
```

### OutcomeWeightMap (ephemeral — built and discarded per rebuild)
```
HashMap<String, f32>   // keyed by phase; per-phase mean of outcome_weight() across all cycles
```

### phase_category_weights return type
```
HashMap<(String, String), f32>   // keyed by (phase, category)
                                  // value = bucket.len() / total_entries_for_phase
                                  // sums to 1.0 per phase; empty when use_fallback = true
```

### InferenceConfig additions
```rust
// Renamed with alias
#[serde(alias = "query_log_lookback_days")]
pub phase_freq_lookback_days: u32,

// New field
pub min_phase_session_pairs: u32,   // default 5, range [1, 1000]
```

### MILLIS_PER_DAY constant
```rust
const MILLIS_PER_DAY: i64 = 86_400 * 1_000;  // ms-epoch scaling; do NOT use 86_400
```

---

## Function Signatures

### New store functions (unimatrix-store)

```rust
// Replaces query_phase_freq_table
pub async fn query_phase_freq_observations(
    &self,
    lookback_days: u32,
) -> Result<Vec<PhaseFreqRow>, StoreError>

// New; Query B result type
pub async fn query_phase_outcome_map(
    &self,
) -> Result<Vec<PhaseOutcomeRow>, StoreError>
```

### New/modified PhaseFreqTable methods (unimatrix-server/src/services/phase_freq_table.rs)

```rust
// New public method — W3-1 GNN cold-start; NOT on search hot path
pub fn phase_category_weights(&self) -> HashMap<(String, String), f32>

// New private function
fn outcome_weight(outcome: &str) -> f32
// "rework" (case-insensitive contains) → 0.5 (checked first)
// "fail"   (case-insensitive contains) → 0.5
// "pass"   (case-insensitive contains) → 1.0
// all other / empty / unknown         → 1.0 (graceful degradation)

// New private function called from rebuild()
fn apply_outcome_weights(
    rows: Vec<PhaseFreqRow>,
    outcome_rows: Vec<PhaseOutcomeRow>,
) -> Vec<PhaseFreqRow>
// Build HashMap<String, f32> (phase → mean weight) from outcome_rows
// Multiply each row.freq by per-phase weight (default 1.0); return modified rows
```

### Deleted

```rust
// DELETED: unimatrix-store
pub async fn query_phase_freq_table(
    &self,
    lookback_days: u32,
) -> Result<Vec<PhaseFreqRow>, StoreError>
```

---

## Query A (canonical SQL)

```sql
SELECT o.phase,
       e.category,
       CAST(json_extract(o.input, '$.id') AS INTEGER) AS entry_id,
       COUNT(*) AS freq
FROM observations o
  JOIN entries e ON CAST(json_extract(o.input, '$.id') AS INTEGER) = e.id
WHERE o.phase IS NOT NULL
  AND o.hook = 'PreToolUse'
  AND o.tool IN ('context_get', 'mcp__unimatrix__context_get',
                 'context_lookup', 'mcp__unimatrix__context_lookup')
  AND json_extract(o.input, '$.id') IS NOT NULL
  AND o.ts_millis > ?1
GROUP BY o.phase, e.category, entry_id
ORDER BY o.phase, e.category, freq DESC
```

`?1` is bound as `cutoff_millis: i64` (pre-computed in Rust using `MILLIS_PER_DAY`).

## Query B (canonical SQL)

```sql
SELECT ce.phase, s.feature_cycle, ce.outcome
FROM cycle_events ce
  JOIN sessions s ON s.feature_cycle = ce.cycle_id
WHERE ce.event_type = 'cycle_phase_end'
  AND ce.phase IS NOT NULL
  AND ce.outcome IS NOT NULL
  AND s.feature_cycle IS NOT NULL
```

---

## Constraints

1. **No schema migration** — all required columns exist: `observations.{session_id, ts_millis, hook, tool, input, phase}`, `entries.{id, category}`, `cycle_events.{cycle_id, event_type, phase, outcome}`, `sessions.feature_cycle`.
2. **CAST mandatory** in JOIN predicate — omitting causes silent zero-row return (col-031 R-05).
3. **`o.hook = 'PreToolUse'`** (not `o.hook_event`) — the DB column is `hook`; `hook_event` does not exist and produces a runtime SQL error (ADR-007).
4. **`cutoff_millis` pre-computed in Rust** using `MILLIS_PER_DAY: i64 = 86_400 * 1_000`; bound as `i64` (not `u32` — sqlx 0.8 INTEGER mapping); bound as `?1` in Query A (ADR-006).
5. **4-entry IN clause** for tool names — no REPLACE/SUBSTR; the `mcp__unimatrix__` prefix is the only hook-listener variant (crt-049 AC-06 confirmed).
6. **Per-phase mean weighting** in `apply_outcome_weights` — aggregate all per-cycle outcome weights for a phase by mean, not best-weight; this preserves rank ordering invariant across mixed-weight buckets (R-03).
7. **`outcome_weight()` priority order**: rework checked before fail — consistent with `infer_gate_result()` in `tools.rs` (ADR-003); doc comment must cross-reference that function.
8. **All existing `PhaseFreqTable` contracts preserved**: rank-based normalization (col-031 ADR-001), `phase_affinity_score()` signature and cold-start 1.0 return, retain-on-error, `PhaseFreqTableHandle` type alias, poison recovery via `unwrap_or_else`.
9. **`phase_category_weights()` uses bucket breadth** (entry count per category), not weighted-freq sum — formula is `bucket.len() / total_entries_for_phase`; doc comment must state this is a breadth-based distribution (R-07).
10. **`min_phase_session_pairs` gate**: when distinct `(phase, session_id)` count in the lookback window falls below this threshold, set `use_fallback = true` and emit `tracing::warn!`; the rebuild stops early.
11. **serde alias only covers TOML deserialization** — all Rust struct literal sites using `query_log_lookback_days:` must be updated to `phase_freq_lookback_days:` (compiler enforces; audit required per SR-04 / ADR-004).
12. **Query B store error must propagate** — both Query A and Query B errors must return `Err` to `rebuild()`; do not silently treat Query B failure as empty outcome map.
13. **`PhaseOutcomeRow` is not re-exported** from `unimatrix-store` crate root (internal to rebuild).

---

## Dependencies

### Crate dependencies (no new crates required)

| Crate | Usage |
|-------|-------|
| `unimatrix-store` | `PhaseFreqRow`, `SqlxStore`, `StoreError`, sqlx 0.8 with sqlite feature |
| `unimatrix-server` | `PhaseFreqTable`, `InferenceConfig`, `status.rs` diagnostics, `background.rs` tick wiring |
| `unimatrix-core` | `Store` trait (unchanged) |

### Feature dependencies (all merged)

| Feature | Provides | Status |
|---------|----------|--------|
| crt-049 (#539) | `observations.input` with explicit read IDs; storage contract confirmed (ADR-005) | Merged (eaed9428, 5a6850db, 813c4801) |
| crt-043 | `observations.phase` column | Merged |
| col-022 | `sessions.feature_cycle` column; pre-col-022 sessions have NULL (FR-10 degradation path) | Merged |
| col-026 | `infer_gate_result()` in `tools.rs` (canonical outcome vocabulary reference) | Merged |
| col-031 | Rank-based normalization formula (ADR-001 #3685), cold-start contracts (ADR-003), `query_log_lookback_days` (ADR-002 #3686) | Merged |
| crt-036 | Tick-time diagnostic for lookback alignment (ADR-003 #3917); must be updated in this feature | Merged |

### External schema columns required (all present)

`observations`: `session_id`, `ts_millis`, `hook`, `tool`, `input`, `phase`
`entries`: `id`, `category`
`cycle_events`: `cycle_id`, `event_type`, `phase`, `outcome`
`sessions`: `feature_cycle`

### Future consumer

ASS-029 / W3-1 GNN will consume `phase_category_weights()`. Cross-crate visibility is a deferred decision (ADR-008 / C-10 in spec).

---

## NOT in Scope

- W3-1 GNN implementation (ASS-029) — only the `phase_category_weights()` accessor.
- Changing `w_phase_explicit` or `w_phase_histogram` default values (W3-1's domain).
- Changing `PhaseFreqTable` internal struct, rank-based scoring formula, or `phase_affinity_score()` signature.
- Adding a new DB table or schema migration.
- Changing how `context_get` or `context_lookup` write observations.
- Changing `explicit_read_by_category` in `FeatureKnowledgeReuse` with a phase dimension.
- Phase-stratified goal-cluster retrieval (deferred in crt-046).
- Removing `query_log` from the codebase — only `query_phase_freq_table` (one function) is deleted.
- Changing `query_log.ts` to millisecond epoch.
- Hardening outcome string vocabulary into an enum.

---

## Alignment Status

Overall: **WARN — three items require human review before delivery begins.**

Vision and milestone alignment: **PASS.** The feature correctly advances Wave 1A signal quality, prepares the W3-1 GNN cold-start path, and does not prematurely implement Wave 2/3 capabilities.

### VARIANCE 1 — Human Approval Required: Coverage threshold gate vs. warning-only

**What**: SCOPE.md AC-11 authorized an observations-coverage *diagnostic warning*. The specification (FR-17, AC-14) and architecture both implement this as a hard `use_fallback = true` gate: when distinct `(phase, session_id)` pairs fall below `min_phase_session_pairs` (default 5), phase scoring is actively disabled. In development and early-deployment environments with sparse observations, this could suppress a valid (if low-confidence) signal.

**Additionally**: RISK-TEST-STRATEGY R-04 identifies a disagreement between architecture (default = 5) and spec suggestion (default = 10) for `min_phase_session_pairs`. The implementer needs a single authoritative value.

**Human decision needed**:
- (a) Confirm hard `use_fallback` gate vs. warning-only. If warning-only, demote FR-17 to emit a warning without setting `use_fallback = true`.
- (b) Confirm the default threshold value: 5 (architecture) or 10 (spec suggestion), or another value.

### VARIANCE 2 — Resolved by brief (field naming inconsistency)

ARCHITECTURE.md uses `min_phase_session_pairs`; SPECIFICATION.md domain models use `min_phase_session_coverage`. The authoritative name is `min_phase_session_pairs` (ARCHITECTURE.md Integration Surface table, which is the implementation contract). Implementers must use `min_phase_session_pairs`.

### VARIANCE 3 — Action required before delivery: Spec C-02/AC-SV-01 incorrect assertion

SPECIFICATION.md C-02, FR-07, and AC-SV-01 contain language describing a double-encoding problem on the hook-listener write path. RISK-TEST-STRATEGY R-01 (highest priority, Critical) and ADR-005 both confirm this is incorrect — the write path produces a plain JSON object string; `json_extract(input, '$.id')` works for all rows. The spec must be corrected before the implementation agent begins work to avoid implementation agent confusion. The write-path contract test (R-01 test scenario 1) is still required and must be included in the test plan.

### VARIANCE 4 — Human Approval Recommended: Per-phase weight aggregation not in ADR table

ARCHITECTURE.md OQ-1 resolves the per-phase weight aggregation as mean-weight but leaves the decision to the implementer with a prose recommendation ("the implementer should choose mean-weight"). This decision is not captured as a named ADR in the Technology Decisions table. RISK-TEST-STRATEGY R-03 confirms that using per-cycle weights (instead of per-phase mean) can scramble rank ordering within buckets. The decision is made (mean-weight) and encoded in ADR-001 prose and ADR-003 step 2; however, it lacks an explicit ADR entry.

**Human decision needed**: Confirm that the mean-weight aggregation strategy is approved as documented in ADR-001 and ADR-003, or request an explicit ADR entry before delivery.

---

## Agent Report

Agent: crt-050-synthesizer
Artifacts produced: IMPLEMENTATION-BRIEF.md, ACCEPTANCE-MAP.md
GH Issue: #542 (pre-existing; body to be updated)
