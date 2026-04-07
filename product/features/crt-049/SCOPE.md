# Knowledge Reuse Metric: Explicit Read Signal (crt-049)

## Problem Statement

`context_cycle_review` reports a "Knowledge Reuse" metric that claims to measure how many distinct knowledge entries were served to agents during a feature cycle. The metric is sourced from `query_log.result_entry_ids` (search exposures) plus `injection_log` (hook-injected entries). This is a weak proxy: an entry appearing in search results does not mean the agent read or used it.

Explicit reads — `context_get` calls (single-ID retrieval) and single-target `context_lookup` calls (where `params.id IS NOT NULL`) — are unambiguous signals of intentional knowledge consumption. These calls are already recorded in the `observations` table as `PreToolUse` events with `observations.input = tool_input` serialized as JSON, so `json_extract(input, '$.id')` reliably extracts the entry ID. The `observations` table also carries `phase` (added in crt-043), enabling future downstream work on phase-conditioned category affinity (ASS-040 Group 10).

The current `compute_knowledge_reuse_for_sessions` function is the only step in the `context_cycle_review` pipeline that does NOT use the already-loaded `attributed` observations slice. It makes a second DB round-trip through `scan_query_log_by_sessions` + `scan_injection_log_by_sessions`. This feature fixes both problems: it replaces the second pass with an observations-derived explicit read signal, and relabels `delivery_count` to accurately reflect what it measures.

Affected population: all agents calling `context_cycle_review` to assess knowledge use effectiveness during a cycle.

## Goals

1. Extract explicit read entry IDs from the already-loaded `attributed` observations: filter for `PreToolUse` events where `tool` matches `context_get` or `context_lookup` and `input["id"]` is present (numeric, non-null), extract the entry ID.
2. Add `explicit_read_count: u64` to `FeatureKnowledgeReuse` — the count of distinct entry IDs explicitly read by agents during the cycle.
3. Rename `delivery_count` to `search_exposure_count` in `FeatureKnowledgeReuse` with a `#[serde(alias = "delivery_count")]` backward-compat alias.
4. Update all rendering logic in `render_knowledge_reuse` to use the new field names and expose `explicit_read_count` in the report output.
5. Eliminate the second DB pass: `compute_knowledge_reuse_for_sessions` continues to load `query_log` + `injection_log` for the existing `search_exposure_count` / `cross_session_count` / `by_category` sub-metrics, but the new `explicit_read_count` and `explicit_read_by_category` are derived from the `attributed` observations slice already held in the caller.
6. Add `explicit_read_by_category: HashMap<String, u64>` to `FeatureKnowledgeReuse`: for each entry ID in the explicit read set, join to `entries.category` via `batch_entry_meta_lookup` and tally counts per category. This is the primary input Group 10 (phase-conditioned category affinity) depends on.
7. Redefine `total_served` as `explicit_read_count + injection_count` (deduplicated across both sources). Search exposures are excluded — appearing in a result set is not being served. Update the display label from "Distinct entries served" to "Entries served to agents (reads + injections)". This is a **semantics change** from the current field definition.
8. Bump `SUMMARY_SCHEMA_VERSION` (currently `2` in `cycle_review_index.rs`) to trigger stale-record advisory for all previously-stored records.

## Non-Goals

- This feature does NOT remove `search_exposure_count` (formerly `delivery_count`), `injection_log`, or `query_log` sourcing. Search exposures remain as a separate sub-metric.
- **`total_served` semantics change**: `total_served` is redefined from "alias of `delivery_count`" to "`explicit_read_count + injection_count` (deduplicated)". Search exposures no longer contribute to `total_served`. Any consumer that relied on `total_served ≈ delivery_count` must account for this change.
- This feature does NOT implement phase-conditioned category affinity learning (ASS-040 Group 10) or extend `PhaseFreqTable`. That work explicitly depends on this feature being shipped first.
- This feature does NOT add any new DB tables or schema migrations. No new columns in `observations`, `query_log`, or `cycle_review_index`.
- This feature does NOT change how `context_get` or `context_lookup` write observations. Their recording path is already correct.
- This feature does NOT deduplicate explicit reads against search exposures. The two metrics are independently meaningful and remain distinct.
- This feature does NOT add `phase` breakdowns to the explicit read count. Phase-stratified aggregates belong to Group 10.
- This feature does NOT extend `cross_session_count` to cover explicit reads. Deferred to a follow-on.
- This feature does NOT touch the `query_log`-based `PhaseFreqTable` (that is Group 10 scope).

## Background Research

### Observations Table Structure

The `observations` table (schema in `crates/unimatrix-store/src/db.rs`) stores:
- `tool TEXT` — tool name as received from the hook (may carry the `mcp__unimatrix__` prefix)
- `input TEXT` — `tool_input` JSON-stringified (for PreToolUse); parsed back to `serde_json::Value` in `services/observation.rs`
- `phase TEXT` — active session phase at write time (added crt-043; NULL when no cycle active)
- `session_id TEXT` — FK to sessions

For `context_get`: `tool_input = {"id": <N>, "feature": "...", "format": "...", ...}`. `json_extract(input, '$.id')` returns a numeric entry ID.

For `context_lookup` (single-ID path, `params.id IS NOT NULL`): same shape — `{"id": <N>, ...}`. For the filter-based path (`params.id IS NULL`), no per-entry attribution is possible; `json_extract(input, '$.id')` returns NULL, which is the correct exclusion predicate.

The `ObservationRecord.input` field (in `unimatrix-core/src/observation.rs`) is `Option<serde_json::Value>`, parsed from the stored string. It is available in-memory in the `attributed` slice before step 13 of `context_cycle_review`.

Tool names in `observations.tool` may be `"context_get"` or `"mcp__unimatrix__context_get"` depending on whether the hook path or a direct MCP call wrote the record. `unimatrix_observe::normalize_tool_name()` handles this stripping. The extraction logic must use `normalize_tool_name` for correct matching (confirmed in `session_metrics.rs`).

### Existing Architecture: `compute_knowledge_reuse_for_sessions`

Lives in `tools.rs` (server-side, per col-020 ADR-001). It is called at step 13–14 of `context_cycle_review` with `session_records: &[SessionRecord]`. It:
1. Builds session ID list from `session_records`
2. Calls `store.scan_query_log_by_sessions()` — returns `Vec<QueryLogRecord>` where `result_entry_ids` is a JSON-encoded `Vec<u64>`
3. Calls `store.scan_injection_log_by_sessions()` — returns `Vec<InjectionLogRecord>` where `entry_id: u64`
4. Delegates to `crate::mcp::knowledge_reuse::compute_knowledge_reuse()` for computation

The `attributed` observations slice (`Vec<ObservationRecord>`) is available in the `context_cycle_review` handler scope at step 13, but is NOT passed into `compute_knowledge_reuse_for_sessions`. This is the root inefficiency.

### `FeatureKnowledgeReuse` Type

Defined in `unimatrix-observe/src/types.rs`. Current fields: `delivery_count`, `cross_session_count`, `by_category`, `category_gaps`, `total_served`, `total_stored`, `cross_feature_reuse`, `intra_cycle_reuse`, `top_cross_feature_entries`.

The `delivery_count` field has an existing `#[serde(alias = "tier1_reuse_count")]` alias (historical). Renaming it to `search_exposure_count` requires a new `#[serde(alias = "delivery_count")]` alias to not break pre-existing stored `cycle_review_index` JSON and any external consumers.

### Rendering in `retrospective.rs`

`render_knowledge_reuse()` currently displays `delivery_count` as "Distinct entries served". After this feature it should label the two signals distinctly: "Search exposures" and "Explicit reads".

### SUMMARY_SCHEMA_VERSION

Currently `2` (bumped in crt-047 in `cycle_review_index.rs`). Adding `explicit_read_count` changes the `RetrospectiveReport` JSON round-trip fidelity (field added). Must be bumped to `3`. Pre-existing stored rows will deserialize with `explicit_read_count = 0` via `#[serde(default)]`.

### query_log Coverage Gap (Entry #3417)

`query_log` only records `context_search` and UDS search-path calls. `context_get` and `context_lookup` write nothing to `query_log`. This is a documented limitation: the new `explicit_read_count` closes this coverage gap by sourcing from `observations` instead.

### Pattern #4178 (cycle_review_index)

"When adding columns computed at review time (not during the cycle), add them to `cycle_review_index`, not `cycle_events`. Bump `SUMMARY_SCHEMA_VERSION`." The `explicit_read_count` field belongs in `FeatureKnowledgeReuse` (embedded in the stored `summary_json`) with a `SUMMARY_SCHEMA_VERSION` bump.

## Proposed Approach

**Step 1 — Add `explicit_read_count` to `FeatureKnowledgeReuse`**
- Add field `explicit_read_count: u64` with `#[serde(default)]`
- Rename `delivery_count` to `search_exposure_count` with `#[serde(alias = "delivery_count", alias = "tier1_reuse_count")]`
- Update `total_served` alias/documentation (it mirrors `search_exposure_count`)

**Step 2 — Add extraction helper in `knowledge_reuse.rs`**
- `fn extract_explicit_read_ids(attributed: &[ObservationRecord]) -> HashSet<u64>`
- Filters for `event_type == "PreToolUse"`, `normalize_tool_name(tool) IN ["context_get", "context_lookup"]`
- `record.input` arrives from the hook listener as `Some(Value::String(raw_json))` (the listener wraps the raw JSON string without parsing it — confirmed by `extract_topic_signal` at `listener.rs:1911`). Direct MCP calls may produce `Some(Value::Object(_))`. Both forms must be handled:
  ```
  let obj = match &record.input {
      Some(Value::Object(_)) => record.input.clone(),
      Some(Value::String(s)) => serde_json::from_str(s).ok(),
      _ => None,
  };
  ```
- Extract entry ID from the parsed object: try `as_u64()` first, then `as_str().and_then(|s| s.parse().ok())` to handle string-form IDs (e.g., `{"id": "42"}`), matching `GetParams` deserializer behavior.

**Step 3 — Thread `attributed` into the computation**
- Extend `compute_knowledge_reuse_for_sessions` signature to accept `attributed: &[ObservationRecord]`
- Call `extract_explicit_read_ids(attributed)` and set `reuse.explicit_read_count`
- Update the call site in `context_cycle_review` step 13–14 to pass `&attributed`

**Step 4 — Add `explicit_read_by_category` via category join**
- After `extract_explicit_read_ids`, call `batch_entry_meta_lookup` on the extracted ID set to retrieve `(id, category)` pairs
- Tally into `explicit_read_by_category: HashMap<String, u64>`
- This join happens inside `compute_knowledge_reuse` alongside the existing DB work

**Step 5 — Redefine `total_served`**
- `total_served = |explicit_reads ∪ injections|` (deduplicated union of entry ID sets)
- Search exposures excluded
- Update `render_knowledge_reuse()`: label becomes "Entries served to agents (reads + injections)"
- Render guard: use `total_served == 0 && search_exposure_count == 0` as the early-return condition. An injection-only cycle (no search, no explicit reads, but injections present) has `total_served > 0` — the current `search_exposure_count == 0 && explicit_read_count == 0` guard would incorrectly suppress rendering for such a cycle.

**Step 6 — Update rendering**
- `render_knowledge_reuse()`: rename existing label to "Search exposures (distinct)", add "Explicit reads (distinct)" line, add "Explicit read categories" breakdown from `explicit_read_by_category`

**Step 7 — Bump SUMMARY_SCHEMA_VERSION to 3**
- Update constant, update forced-value assertion in test `CRS-V24-U-01` to `3`

## Acceptance Criteria

- AC-01: `FeatureKnowledgeReuse` has a field `explicit_read_count: u64` with `#[serde(default)]`.
- AC-02: `delivery_count` is renamed to `search_exposure_count` with `#[serde(alias = "delivery_count")]` preserving JSON backward compatibility.
- AC-03: `extract_explicit_read_ids` returns the set of distinct entry IDs from `PreToolUse` observations where the normalized tool name is `context_get` or `context_lookup` and `input["id"]` is a non-null integer or parseable string. The helper handles `record.input` as both `Value::String(raw_json)` (hook-listener path) and `Value::Object(_)` (direct MCP path).
- AC-04: Filter-based `context_lookup` calls (no `id` field in `input`) are excluded from `explicit_read_count`.
- AC-05: `compute_knowledge_reuse_for_sessions` accepts and uses the `attributed` observation slice to populate `explicit_read_count`; the field is present and non-zero when agents made explicit reads during the cycle.
- AC-06: Tool names with the `mcp__unimatrix__` prefix (e.g., `mcp__unimatrix__context_get`) are correctly handled by calling `normalize_tool_name` before matching.
- AC-07: `render_knowledge_reuse` outputs separate labeled lines for "Search exposures" and "Explicit reads" when either is non-zero.
- AC-08: `SUMMARY_SCHEMA_VERSION` is bumped to `3`; the constant assertion test is updated.
- AC-09: A cycle with zero search exposures but non-zero explicit reads does not hit the early-return zero-delivery path.
- AC-10: Existing tests for `FeatureKnowledgeReuse` serialization round-trip continue to pass with the renamed field (alias compatibility verified).
- AC-11: The `compute_knowledge_reuse_for_sessions` still loads `query_log` and `injection_log` records and populates `search_exposure_count`, `cross_session_count`, `by_category`, `cross_feature_reuse`, `intra_cycle_reuse`, and `top_cross_feature_entries` as before.
- AC-12: New unit tests cover: (a) explicit reads extracted from `context_get` observations, (b) filter `context_lookup` excluded, (c) single-ID `context_lookup` included, (d) prefixed tool names handled, (e) empty observations produces `explicit_read_count = 0`.
- AC-13: `FeatureKnowledgeReuse` has a field `explicit_read_by_category: HashMap<String, u64>` with `#[serde(default)]`. It is populated by joining the explicit read entry ID set against `entries.category` via `batch_entry_meta_lookup` and tallying counts per category string. This field is a cycle-level category breakdown (no phase dimension) used as a human-facing reporting and correctness signal. It is NOT the training input for Group 10 — Group 10 requires phase-stratified `(phase, category)` aggregates from `observations` directly (out of scope for crt-049 per C-08). The field name, type, and join source are frozen as a [GATE] contract so Group 10 can consume it without a redesign.
- AC-14: `total_served` is redefined as `explicit_read_count + injection_count` (deduplicated across both sources). Search exposures do NOT contribute to `total_served`. The display label is updated to "Entries served to agents (reads + injections)".
- AC-15: A unit test verifies `total_served` is the count of distinct entry IDs appearing in either explicit reads or injections, not search exposures.
- AC-16: `extract_explicit_read_ids` correctly handles string-form IDs (e.g., `{"id": "42"}`) in addition to integer-form (`{"id": 42}`), matching `GetParams` deserializer behavior. A unit test covers both forms.
- AC-17: An injection-only cycle (injections present, zero search exposures, zero explicit reads) does NOT hit the early-return render guard; `render_knowledge_reuse` produces output showing `total_served > 0`. The render guard condition is `total_served == 0 && search_exposure_count == 0`, not `search_exposure_count == 0 && explicit_read_count == 0`.

## Constraints

- No schema migration: the `observations` table already has `input` and `phase`. No new columns.
- `serde` alias chain on `search_exposure_count` must carry both `"delivery_count"` and `"tier1_reuse_count"` aliases to not break deserialization of any stored `cycle_review_index.summary_json` rows.
- `SUMMARY_SCHEMA_VERSION` bump is mandatory (pattern #4178) to invalidate stale memoized records; skipping it would silently return old records without `explicit_read_count`.
- `FeatureKnowledgeReuse` is defined in `unimatrix-observe` (cross-crate type). The `ObservationRecord` type is in `unimatrix-core`, which `unimatrix-observe` already depends on. The extraction helper in `knowledge_reuse.rs` (server crate) receives `&[ObservationRecord]` — no new crate dependencies required.
- The `compute_knowledge_reuse` function signature in `knowledge_reuse.rs` takes `query_log_records` and `injection_log_records` slices. Adding `attributed` to the public function signature changes the API used only within the server crate — no external API break.
- `normalize_tool_name` must be applied to `observations.tool` before matching: production hook events carry the `mcp__unimatrix__` prefix (confirmed in `session_metrics.rs` and `test_context_cycle_review_curation_health_present_on_cold_start`).

## Open Questions — Resolved

- OQ-01 (resolved): `total_served = explicit_read_count + injection_count` (deduplicated). Search exposures excluded — appearing in a result list is not being served. Display label updated to "Entries served to agents (reads + injections)". See Goal 7 and AC-14/AC-15.
- OQ-02 (resolved): `by_category` (search-exposure sourced) is kept as-is, relabeled "Search exposure categories". A new required field `explicit_read_by_category: HashMap<String, u64>` is added, populated via `batch_entry_meta_lookup` category join. See Goal 6 and AC-13.
- OQ-03 (resolved): `cross_session_count` extension deferred. Out of scope for crt-049.

## Tracking

GH Issue: #539
