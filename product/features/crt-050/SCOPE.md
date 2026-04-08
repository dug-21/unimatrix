# Phase-Conditioned Category Affinity (crt-050)

## Problem Statement

`PhaseFreqTable` is rebuilt every background tick from `query_log` search exposures: entries
that appeared in a search result set during a given phase. A search exposure is not a read
— agents rarely open every result returned by `context_search`. This means the per-(phase,
category) frequency counts conflate "surfaced" with "used", producing a noisy signal that
rewards popular entries regardless of whether they were actually consumed.

crt-049 (GH #539) shipped explicit read signal: `observations.input` now carries the entry
ID for every `context_get` and single-target `context_lookup` PreToolUse event, and
`observations.phase` (crt-043) records the active phase at write time. These two columns
provide a clean `(phase, category)` aggregate — entries that agents deliberately fetched,
stratified by workflow phase — but `PhaseFreqTable::rebuild()` does not yet use them.

The secondary gap is outcome blindness: a phase that required rework (gate failure, retry)
produced weaker learning signal than one that passed cleanly, but both contribute equally to
the current counts. `cycle_events.cycle_phase_end` rows carry `phase` and `outcome` fields
that encode this distinction.

Affected population: every agent call that triggers `context_briefing` or fused re-ranking
where `w_phase_explicit > 0.0` (default 0.05, col-031). That is the live production path.

The W3-1 GNN spike (ASS-029) is deferred until Group 10 validates signal quality in
production. The learned `(phase, category)` weights produced here become W3-1's cold-start
initialization vector, replacing the hand-tuned WA-2 constants (`w_phase_histogram = 0.02`,
`w_phase_explicit = 0.05`).

## Goals

1. Replace `query_log`-sourced `PhaseFreqTable::rebuild()` SQL with an `observations`-sourced
   query: aggregate `(phase, category)` counts from `context_get` + single-ID
   `context_lookup` `PreToolUse` rows joined to `entries.category`.
2. Apply outcome-based weighting from `cycle_events.cycle_phase_end` rows: weight = 1.0 for
   outcomes containing "pass" (case-insensitive), 0.5 for "rework" or "fail". Degrade
   gracefully to unweighted frequency counts when no `cycle_phase_end` history is available
   for a cycle.
3. Preserve all existing `PhaseFreqTable` contracts intact: rank-based normalization
   (col-031 ADR-001), two cold-start caller contracts (col-031 ADR-003), retain-on-error
   semantics, poison recovery, `use_fallback = true` on empty rebuild result.
4. Expose the learned `(phase, category)` weight map as a stable accessor that the future
   W3-1 GNN cold-start initialization path can consume, replacing hand-tuned WA-2 constants.
5. Delete or retain (as a dead config field) `query_log_lookback_days` from `InferenceConfig`
   once `query_log` is no longer the rebuild source. The crt-036 tick-time diagnostic warning
   (ADR-003 #3917) that cross-references this field must be updated or removed accordingly.

## Non-Goals

- This feature does NOT implement the W3-1 GNN itself. ASS-029 runs in parallel. The only
  GNN deliverable here is the accessor method (Goal 4) so W3-1 can read the learned weights.
- This feature does NOT change `PhaseFreqTable`'s in-memory structure, rank-based scoring
  formula, or the `phase_affinity_score()` method signature. Callers (fused scoring, PPR)
  are unchanged.
- This feature does NOT remove `query_log`, `query_log_lookback_days` from the codebase
  unless a clean removal is trivial. Retention or deprecation of the config field is
  implementation-time discretion; the field must not cause a correctness regression.
- This feature does NOT extend `PhaseFreqTable` to store per-entry explicit read counts or
  any other entry-level metric. The training target remains `(phase, category)` aggregates
  only — entry IDs are the lookup key to derive category, never a stored signal.
- This feature does NOT add a new DB table or schema migration. All raw material exists:
  `observations.phase` (crt-043), `observations.input` (crt-049 / #539), `observations.tool`,
  `entries.category`. No new columns are required.
- This feature does NOT change how `context_get` or `context_lookup` write observations.
- This feature does NOT change `w_phase_explicit` or `w_phase_histogram` default values.
  Those are W3-1's domain once the GNN ships.
- This feature does NOT extend `explicit_read_by_category` in `FeatureKnowledgeReuse` with
  a phase dimension. That field (added in crt-049) is cycle-scoped and phase-unaware by
  design; phase-stratified aggregates are PhaseFreqTable's responsibility.
- This feature does NOT implement phase-stratified goal-cluster retrieval (deferred in
  crt-046 scoping notes).

## Background Research

### Current PhaseFreqTable::rebuild() — Query and Signal

`PhaseFreqTable::rebuild()` in `crates/unimatrix-server/src/services/phase_freq_table.rs`
calls `store.query_phase_freq_table(lookback_days)` which executes:

```sql
SELECT q.phase, e.category, CAST(je.value AS INTEGER) AS entry_id, COUNT(*) AS freq
FROM query_log q
  CROSS JOIN json_each(q.result_entry_ids) AS je
  JOIN entries e ON CAST(je.value AS INTEGER) = e.id
WHERE q.phase IS NOT NULL
  AND q.result_entry_ids IS NOT NULL
  AND q.ts > strftime('%s', 'now') - ?1 * 86400
GROUP BY q.phase, e.category, CAST(je.value AS INTEGER)
ORDER BY q.phase, e.category, freq DESC
```

This returns `(phase, category, entry_id, freq)` rows. The per-entry `freq` (count of
appearances in result sets) is used only as a rank-ordering key — higher freq → lower rank
index → higher affinity score. The actual score is rank-based (col-031 ADR-001), not
frequency-proportional.

The crt-036 ADR-003 (entry #3917) added a tick-time diagnostic warning when
`query_log_lookback_days` exceeds the oldest retained cycle's age. This warning must be
evaluated against the new signal source.

### Explicit Read Signal — What crt-049 Delivered

`observations` schema (db.rs):
- `tool TEXT` — may carry `mcp__unimatrix__` prefix; `normalize_tool_name()` strips it
- `input TEXT` — `tool_input` JSON; `json_extract(input, '$.id')` yields entry ID for
  `context_get` and single-ID `context_lookup`
- `phase TEXT` — active phase at write time (NULL when no cycle active; crt-043)
- `session_id TEXT` — FK to sessions

`crt-049` proved the extraction at the Rust layer (`extract_explicit_read_ids` in
`knowledge_reuse.rs`), but the PhaseFreqTable rebuild needs a SQL-side aggregation, not
an in-memory slice filter. The equivalent SQL join would be:

```sql
SELECT o.phase,
       e.category,
       CAST(json_extract(o.input, '$.id') AS INTEGER) AS entry_id,
       COUNT(*) AS freq          -- used only for rank ordering
FROM observations o
  JOIN entries e ON CAST(json_extract(o.input, '$.id') AS INTEGER) = e.id
WHERE o.phase IS NOT NULL
  AND o.tool IN ('context_get', 'mcp__unimatrix__context_get',
                 'context_lookup', 'mcp__unimatrix__context_lookup')
  AND json_extract(o.input, '$.id') IS NOT NULL
  AND o.ts_millis > (strftime('%s', 'now') - ?1 * 86400) * 1000
GROUP BY o.phase, e.category, entry_id
ORDER BY o.phase, e.category, freq DESC
```

The `json_extract` + `CAST AS INTEGER` pattern mirrors the existing `query_phase_freq_table`
CAST form (the CAST is mandatory — omitting it causes text-integer JOIN mismatch returning
zero rows, per col-031 R-05). The `mcp__unimatrix__` prefix variants must be included
because hook-path observations carry the prefixed tool name (confirmed in crt-049 / AC-06).

Key difference from `query_log`: `observations.ts_millis` is millisecond-epoch while
`query_log.ts` is second-epoch. The window cutoff must multiply by 1000, or a separate
`observations`-appropriate lookback parameter can be introduced.

### Outcome Weighting — cycle_events Source

`cycle_events` schema (db.rs):
```
cycle_id TEXT, seq INTEGER, event_type TEXT, phase TEXT,
outcome TEXT, next_phase TEXT, timestamp INTEGER, goal TEXT, goal_embedding BLOB
```

Phase transition events use `event_type = 'cycle_phase_end'` with `phase` = the phase that
just ended and `outcome` containing free-text outcome strings (e.g., "PASS", "fail",
"REWORK"). The `infer_gate_result()` function in `tools.rs` (col-026 R-03) already parses
these with substring matching: Rework > Fail > Pass > Unknown, case-insensitive contains.

For weighting: a `(phase, cycle_id)` pair where `cycle_phase_end.outcome` contains "pass"
should weight 1.0; "rework" or "fail" should weight 0.5. A cycle with no `cycle_phase_end`
rows for a given phase (in-progress cycle, or cycle that completed without emitting phase
events) falls through to unweighted count (weight = 1.0 by default).

The `sessions` table also carries `outcome TEXT` ("success" | "rework" | "abandoned") and
is already used in `read.rs` for injection outcome weighting. However, `sessions.outcome`
is session-level, not phase-level — the correct join for per-phase weighting is
`cycle_events` directly. Phase-level outcome from `cycle_phase_end.outcome` is more precise
than session-level outcome.

### PhaseFreqTable Integration Points

Four integration points are unchanged:
1. `PhaseFreqTable::rebuild()` — only the store call and SQL change
2. `PhaseFreqTable::phase_affinity_score()` — unchanged; rank-based scoring preserved
3. `PhaseFreqTable::new_handle()`, `PhaseFreqTable::new()` — unchanged; cold-start contract preserved
4. `run_single_tick` wiring in `background.rs` — unchanged; rebuild is still triggered each tick

The existing `PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>` type alias and poison
recovery pattern (`unwrap_or_else(|e| e.into_inner())`) are unchanged.

### W3-1 GNN Cold-Start Interface

W3-1 requires a `HashMap<(phase_string, category_string), f32>` — the post-normalization
weight map that represents learned affinity for each (phase, category) pair. Currently
the internal `table` field stores `HashMap<(String, String), Vec<(u64, f32)>>` (per-entry
rank scores), not the aggregate category-level weights directly.

The GNN cold-start initialization needs a different projection of this data: for each
`(phase, category)` bucket, the bucket's "weight" is a scalar summarizing how strongly that
phase uses that category — for example, the mean rank score across the bucket, or the
normalized bucket size relative to other categories in the same phase. The exact
aggregation strategy is a design-time decision (open question OQ-03 below).

A new public method `phase_category_weights(&self) -> HashMap<(String, String), f32>` (or
similar) would project the internal table into the GNN-consumable form. This method is not
called on the search hot path; it is called at GNN initialization time only.

### WA-2 Constants Being Replaced (Long-Term)

The current hand-tuned constants from ASS-028 and col-031:
- `w_phase_histogram = 0.02` (default, `InferenceConfig`) — session histogram affinity term
- `w_phase_explicit = 0.05` (default, `InferenceConfig`) — PhaseFreqTable affinity term
- `default_w_phase_explicit()` and `default_w_phase_histogram()` in `config.rs`

W3-1 is expected to replace these with learned values after training on the explicit read
signal validated by this feature. crt-050 does not change the default values — it only
improves the quality of the signal that feeds the table those values score against. The
actual weight constants remain for W3-1 to update.

### Existing `PhaseFreqRow` Type

`PhaseFreqRow` in `unimatrix-store/src/query_log.rs` carries `(phase, category, entry_id,
freq)`. This row type is re-exported from `unimatrix-store`'s crate root. The new SQL query
returns the same shape — the row type can be reused. The deserializer
`row_to_phase_freq_row` may need to change if the SQL column ordering changes.

### crt-036 Lookback Days Dependency

`query_log_lookback_days: u32` in `InferenceConfig` (col-031 ADR-002 #3686) governs the
`PhaseFreqTable` time window. If `query_log` is no longer the rebuild source, this field
becomes semantically misnamed. Options at implementation time:
- Rename to `phase_freq_lookback_days` with a serde alias for backward compat
- Retain `query_log_lookback_days` as the shared field (acceptable — the meaning is "how
  far back to look for phase-category signal")
- Add a new `explicit_read_lookback_days` field and deprecate `query_log_lookback_days`

The crt-036 tick-time diagnostic warning (ADR-003, entry #3917) that checks whether
`query_log_lookback_days` exceeds the oldest retained cycle's age must be reviewed at
implementation time: it may be obsolete if the new source (observations) is not pruned by
the same K-cycle retention logic.

## Proposed Approach

**Step 1 — Two new store queries: base aggregate + outcome map**

Add two async fns on `SqlxStore` in `query_log.rs` (or a new `phase_freq.rs` module):

*Query A — base observations aggregate:*

```sql
SELECT o.phase,
       e.category,
       CAST(json_extract(o.input, '$.id') AS INTEGER) AS entry_id,
       COUNT(*) AS freq
FROM observations o
  JOIN entries e ON CAST(json_extract(o.input, '$.id') AS INTEGER) = e.id
WHERE o.phase IS NOT NULL
  AND o.hook_event = 'PreToolUse'
  AND o.tool IN ('context_get', 'mcp__unimatrix__context_get',
                 'context_lookup', 'mcp__unimatrix__context_lookup')
  AND json_extract(o.input, '$.id') IS NOT NULL
  AND o.ts_millis > (strftime('%s', 'now') - ?1 * 86400) * 1000
GROUP BY o.phase, e.category, entry_id
ORDER BY o.phase, e.category, freq DESC
```

Returns `Vec<PhaseFreqRow>` — same type as today; no change to callers.

*Query B — outcome map:*

```sql
SELECT ce.phase, s.feature_cycle, ce.outcome
FROM cycle_events ce
  JOIN sessions s ON s.feature_cycle = ce.cycle_id
WHERE ce.event_type = 'cycle_phase_end'
  AND ce.phase IS NOT NULL
  AND ce.outcome IS NOT NULL
```

Returns `Vec<(phase, feature_cycle, outcome)>`. Consumed in Rust to build a
`HashMap<(phase, feature_cycle), f32>` by calling the existing `infer_gate_result()`
logic: "pass" (case-insensitive contains) → 1.0, "rework" or "fail" → 0.5. Phases with
no matching row default to 1.0 (unweighted). The Rust post-process multiplies each row's
`freq` from Query A by the corresponding `(phase, session_cycle_id)` weight.

**Rationale for two-query approach (OQ-01)**: The join path
`observations.session_id → sessions.feature_cycle → cycle_events` spans three tables at
different granularities (observation-level vs. cycle-level). A single SQL query combining
these is difficult to test in isolation. Two queries allow the weighting function to be
tested independently with synthetic outcome data. The existing `infer_gate_result()` in
`tools.rs` already implements case-insensitive substring matching — the Rust post-process
calls it directly rather than reimplementing it in SQL CASE expressions.

**Step 2 — Update `PhaseFreqTable::rebuild()` to call the new queries**

Replace the `store.query_phase_freq_table(lookback_days)` call with the two-query path.
All downstream logic (grouping, rank normalization, cold-start handling) is unchanged.
Delete `query_phase_freq_table` (old fn) — it has exactly one call site, no external
consumers, and its tests must be rewritten for the new signal source anyway.

**Step 3 — Add `phase_category_weights()` accessor on `PhaseFreqTable`**

New public method returning `HashMap<(String, String), f32>`. Aggregation strategy
(OQ-02): normalized bucket size — for each `(phase, category)` bucket, the weight is the
fraction of total explicit reads for that phase attributable to that category. This forms
a probability distribution over categories per phase, summing to 1.0 within each phase,
directly answering "given phase P, how likely is category C to be useful?". Returns an
empty map when `use_fallback = true`. Formula documented in the implementing ADR.

**Step 4 — Rename `query_log_lookback_days` with serde alias**

Rename `InferenceConfig::query_log_lookback_days` → `phase_freq_lookback_days` with
`#[serde(alias = "query_log_lookback_days")]`. The old name is actively misleading once
`query_log` is no longer the signal source. The serde alias handles all existing config
files transparently. Update the crt-036 ADR-003 tick-time diagnostic to reference the new
field name and note that it now governs the observations window, not `query_log`. Add a
parallel observations-coverage diagnostic at tick time: when the distinct `(phase,
session)` count within the lookback window falls below `min_phase_session_pairs` (default 5),
  set `use_fallback = true` and emit a tick-time warning.

**Rationale for SQL-side base aggregate**: SQL-side grouping for the base counts is
correct — the explicit read signal has higher cardinality than `query_log` (one row per
read vs. one row per search result set), making SQL-side aggregation more important, not
less. The weighting step (Rust post-process) operates on the already-aggregated
`Vec<PhaseFreqRow>`, not on raw observation rows, keeping the Rust-side work proportional
to `(phase × category × entry)` buckets, not raw row counts.

**Rationale for reusing `PhaseFreqRow`**: The row shape `(phase, category, entry_id,
freq)` is unchanged. `freq` is now an outcome-weighted count rather than a raw exposure
count, but the rank-normalization formula (col-031 ADR-001) operates on the ordering of
`freq` values within a bucket, not their absolute magnitudes — the formula is invariant to
this change.

## Acceptance Criteria

- AC-01: `PhaseFreqTable::rebuild()` sources its data from `observations`-joined
  `(phase, category, entry_id, freq)` aggregates, not `query_log`.
- AC-02: The base aggregate query filters to `context_get` and `context_lookup` tool names,
  handling both bare names and `mcp__unimatrix__`-prefixed variants, with
  `json_extract(input, '$.id') IS NOT NULL` as the single-ID predicate, and
  `hook_event = 'PreToolUse'` to exclude PostToolUse rows that would otherwise double-count.
- AC-03: `CAST(json_extract(input, '$.id') AS INTEGER)` is used for the entries JOIN —
  the CAST form is mandatory to prevent silent zero-row returns (col-031 R-05).
- AC-04: Outcome weighting is applied via a two-query Rust post-process. Query B fetches
  `(phase, feature_cycle, outcome)` from `cycle_events` joined to `sessions`. The Rust
  post-process calls `infer_gate_result()` for substring matching: "pass" → 1.0,
  "rework"/"fail" → 0.5. The resulting weighted `freq` values are used in Query A's rows.
- AC-05: When no `cycle_phase_end` history is available for a phase (cold-start, in-progress
  cycle, or cycle with no phase events), the query degrades to unweighted frequency counts
  (weight = 1.0 for all rows) with no error and no use_fallback escalation.
- AC-06: All existing `PhaseFreqTable` contracts are preserved: rank-based normalization
  formula (col-031 ADR-001), `phase_affinity_score()` returns 1.0 for cold-start / absent
  phase / absent entry (col-031 ADR-003), retain-on-error semantics (R-09), poison recovery
  via `unwrap_or_else`.
- AC-07: The time window filter uses `observations.ts_millis` (millisecond epoch); the
  lookback boundary is computed correctly accounting for the ms-vs-seconds unit difference
  relative to `query_log.ts`.
- AC-08: `PhaseFreqTable` exposes a `phase_category_weights()` method (or equivalent) that
  returns a `HashMap<(String, String), f32>` summarizing the learned per-`(phase, category)`
  affinity for use as a W3-1 GNN cold-start initialization vector. Returns an empty map
  when `use_fallback = true`.
- AC-09: `query_phase_freq_table` (old fn) is deleted. It has exactly one call site
  (being replaced) and no external consumers. Its tests must be rewritten for the new
  signal source; retaining dead code would create misleading ambiguity about the active path.
- AC-10: `query_log_lookback_days` in `InferenceConfig` is renamed to
  `phase_freq_lookback_days` with `#[serde(alias = "query_log_lookback_days")]`. The old
  name is actively misleading once `query_log` is not the signal source.
- AC-11: The crt-036 tick-time diagnostic warning (ADR-003 #3917) is updated to reference
  `phase_freq_lookback_days` and note that it governs the `observations` window. A parallel
  observations-coverage diagnostic is added: when distinct `(phase, session)` count within
  the lookback window falls below `min_phase_session_pairs` (default 5), set
  `use_fallback = true` and emit a tick-time warning.
- AC-12: Eval harness regression gate passes: MRR does not decrease below the 0.2788
  baseline (post-PPR-expander). The behavioral scenario set
  (`product/research/ass-039/harness/scenarios.jsonl`, 1,761 scenarios) is the canonical
  measurement instrument.
- AC-13: Unit tests cover: (a) empty observations → `use_fallback = true`; (b) pass-outcome
  rows weighted 1.0; (c) rework/fail-outcome rows weighted 0.5; (d) missing outcome
  degrades to unweighted counts; (e) prefixed tool names included; (f) filter-based
  `context_lookup` (no `$.id`) excluded; (g) `phase_category_weights()` returns empty map
  on cold-start and non-empty map on populated table.

## Constraints

- No schema migration: `observations.phase` (crt-043), `observations.input` (shipped),
  `entries.category`, and `cycle_events` are all present. No new columns needed.
- **Critical (Gap 1) — `observations.input` storage contract**: `json_extract(o.input,
  '$.id')` is only valid if `observations.input` stores a normalized JSON object in all
  write paths. The hook-listener path in `listener.rs` wraps `input_str` as
  `serde_json::Value::String(...)`, which serializes as a double-encoded string — double
  encoding causes `json_extract` to return NULL for all hook-path rows silently. This
  feature depends on crt-049's Issue 1 being resolved as storage normalization (JSON object
  written at all write paths), not just Rust-layer extraction. If crt-049 resolves Issue 1
  at the Rust layer only (without normalizing storage), the SQL approach is invalid for
  hook-path observations and the architect must use a two-phase approach (SQL for direct-MCP
  rows + Rust for hook-path rows) instead. Confirm the crt-049 storage contract before
  implementing Step 1.
- **Gap 2 — CAST handles both ID forms**: `CAST(json_extract(input, '$.id') AS INTEGER)`
  correctly handles both integer-form (`{"id": 42}`) and string-form (`{"id": "42"}`) IDs —
  `CAST('42' AS INTEGER) = 42` in SQLite — providing equivalent coverage to crt-049 AC-16
  at the SQL layer. This is intentional, not incidental.
- **Gap 3 — PreToolUse filter required**: The base aggregate SQL must include
  `hook_event = 'PreToolUse'` (or the equivalent column name in the `observations` schema).
  Without this filter, any PostToolUse rows for `context_get`/`context_lookup` would
  double-count each explicit read. The SQL in Proposed Approach Step 1 includes this filter.
- `CAST(json_extract(input, '$.id') AS INTEGER)` is mandatory in the JOIN predicate —
  omitting it causes a text-to-integer mismatch returning zero rows silently (col-031 R-05,
  documented in `query_log.rs`).
- `observations.ts_millis` is millisecond-epoch; `query_log.ts` is second-epoch. The
  lookback window boundary formula must account for this difference (multiply seconds × 1000).
- The `mcp__unimatrix__` prefix is the only prefix variant written by the hook-listener path
  — confirmed during crt-049 research. No `unimatrix__context_get` variant (without `mcp__`)
  exists. The 4-entry IN clause is the correct and complete approach; using SQLite
  `REPLACE`/`SUBSTR` instead would be less explicit and less indexable.
- `PhaseFreqRow` is declared in `unimatrix-store/src/query_log.rs` and re-exported from the
  crate root. If the new query lives in a new module, `PhaseFreqRow` remains in its current
  location (or is moved to a shared location). No type signature changes on any public fn.
- crt-049 (#539) must be merged before this feature ships. At scope time, crt-049 is merged
  (GH #539 referenced in git log as eaed9428, 5a6850db, 813c4801). The storage contract for
  `observations.input` (Gap 1 above) must be verified against the merged code.
- The eval regression gate (AC-12, MRR ≥ 0.2788) is a hard gate. Phase signal change is
  expected to produce a MRR improvement or neutral result; a regression blocks merge.
- `phase_category_weights()` is `pub` on `PhaseFreqTable` for W3-1 access. `PhaseFreqTable`
  lives in `unimatrix-server/src/services/` — if W3-1 needs this from a different crate,
  visibility must be reviewed at that time.

## Open Questions

All OQs resolved. No open questions remain.

## Tracking

GH Issue: #542
