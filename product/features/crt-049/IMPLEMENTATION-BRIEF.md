# Implementation Brief: crt-049 — Knowledge Reuse Metric: Explicit Read Signal

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-049/SCOPE.md |
| Architecture | product/features/crt-049/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-049/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-049/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-049/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| FeatureKnowledgeReuse (types.rs) | pseudocode/feature-knowledge-reuse.md | test-plan/feature-knowledge-reuse.md |
| extract_explicit_read_ids (knowledge_reuse.rs) | pseudocode/extract-explicit-read-ids.md | test-plan/extract-explicit-read-ids.md |
| compute_knowledge_reuse (knowledge_reuse.rs) | pseudocode/compute-knowledge-reuse.md | test-plan/compute-knowledge-reuse.md |
| compute_knowledge_reuse_for_sessions (tools.rs) | pseudocode/compute-knowledge-reuse-for-sessions.md | test-plan/compute-knowledge-reuse-for-sessions.md |
| render_knowledge_reuse (retrospective.rs) | pseudocode/render-knowledge-reuse.md | test-plan/render-knowledge-reuse.md |
| SUMMARY_SCHEMA_VERSION (cycle_review_index.rs) | pseudocode/schema-version-bump.md | test-plan/schema-version-bump.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Replace the weak search-exposure proxy in `context_cycle_review`'s knowledge reuse metric with an explicit read signal derived from `context_get` and single-ID `context_lookup` observations already loaded in memory, adding `explicit_read_count` and `explicit_read_by_category` to `FeatureKnowledgeReuse`, redefining `total_served` as the deduplicated union of explicit reads and injections (excluding search exposures), renaming `delivery_count` to `search_exposure_count` with full serde alias backward compatibility, and bumping `SUMMARY_SCHEMA_VERSION` to `3`. This eliminates a second DB round-trip in the review pipeline and provides the consumption-side signal that ASS-040 Group 10 (phase-conditioned category affinity) depends on.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Placement of extraction helper | Standalone `pub(crate) fn extract_explicit_read_ids` in `knowledge_reuse.rs` — pure function over in-memory slice, unit testable without a store fixture; aligns with col-020 ADR-001 compute/IO separation | Architecture Component 2; ADR-001 | architecture/ADR-001-extract-explicit-read-ids-helper.md |
| Serde alias strategy for `search_exposure_count` | Stacked `#[serde(alias)]` attributes (one per line) — portable across serde versions; canonical field name `search_exposure_count`; aliases `delivery_count` and `tier1_reuse_count` both required | Architecture Component 1; ADR-002 | architecture/ADR-002-triple-alias-serde-chain.md |
| `total_served` redefinition | `total_served = \|explicit_read_ids ∪ injection_ids\|` (deduplicated set union); search exposures excluded; consumer inventory confirmed zero external consumers reading `total_served` for business logic | Architecture Component 2; ADR-003 | architecture/ADR-003-total-served-redefinition.md |
| Cardinality cap on `batch_entry_meta_lookup` for explicit reads | Cap at 500 IDs before the batch lookup; `explicit_read_count` computed from full uncapped set; `tracing::warn` emitted when cap is hit; `explicit_read_by_category` is partial above cap (documented) | Architecture Integration Points; ADR-004 | architecture/ADR-004-explicit-read-batch-lookup-cardinality.md |

---

## Files to Create / Modify

| File | Change |
|------|--------|
| `crates/unimatrix-observe/src/types.rs` | Rename `delivery_count` to `search_exposure_count` with stacked serde aliases; add `explicit_read_count: u64` and `explicit_read_by_category: HashMap<String, u64>` with `#[serde(default)]`; update `total_served` doc comment |
| `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` | Add `extract_explicit_read_ids` helper; extend `compute_knowledge_reuse` signature with `explicit_read_ids: &HashSet<u64>` and `explicit_read_meta: &HashMap<u64, EntryMeta>`; compute `explicit_read_count`, `explicit_read_by_category`, and redefined `total_served`; update early-return guard |
| `crates/unimatrix-server/src/tools.rs` | Add `attributed: &[ObservationRecord]` parameter to `compute_knowledge_reuse_for_sessions`; call `extract_explicit_read_ids`; call `batch_entry_meta_lookup` for explicit read IDs (with 500-ID cap via `EXPLICIT_READ_META_CAP`); pass results into `compute_knowledge_reuse`; update call site at step 13-14 of `context_cycle_review` to pass `&attributed` |
| `crates/unimatrix-server/src/mcp/response/retrospective.rs` | Update `render_knowledge_reuse`: correct early-return guard to `total_served == 0 && search_exposure_count == 0`; add "Search exposures (distinct)" and "Explicit reads (distinct)" labeled lines; rename summary label to "Entries served to agents (reads + injections)"; add "Explicit read categories" breakdown |
| `crates/unimatrix-store/src/cycle_review_index.rs` | Bump `SUMMARY_SCHEMA_VERSION` from `2` to `3`; update advisory message text to name the `total_served` semantic change specifically; update `CRS-V24-U-01` assertion from `2` to `3` |

Test modules updated (extend existing, do not create isolated scaffolding):
- `crates/unimatrix-observe/src/types.rs` test module: alias round-trip tests for all three `search_exposure_count` alias names (AC-02)
- `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` test module: AC-12 unit tests (a)-(e), AC-16 string-form ID test
- `crates/unimatrix-server/src/tools.rs` test module: update `test_compute_knowledge_reuse_for_sessions_no_block_on_panic` to pass `&[]` for `attributed`; add AC-05 integration test and AC-07 golden render assertion
- `crates/unimatrix-store/src/cycle_review_index.rs` test module: update `CRS-V24-U-01` to assert version `3`

---

## Data Structures

### FeatureKnowledgeReuse (after crt-049) — `unimatrix-observe/src/types.rs`

```rust
pub struct FeatureKnowledgeReuse {
    // Renamed from delivery_count (crt-049). Both aliases required for stored row compat.
    #[serde(alias = "delivery_count")]
    #[serde(alias = "tier1_reuse_count")]
    pub search_exposure_count: u64,

    // New: distinct entry IDs retrieved by agents via context_get / single-ID context_lookup
    #[serde(default)]
    pub explicit_read_count: u64,

    // New: per-category tally of explicit reads; populated via batch_entry_meta_lookup.
    // Cycle-level breakdown only (no phase dimension).
    // NOT the Group 10 training input — Group 10 requires phase-stratified (phase, category)
    // aggregates from observations directly (out of scope, C-08). [GATE contract AC-13]
    #[serde(default)]
    pub explicit_read_by_category: HashMap<String, u64>,

    // Redefined (crt-049): |explicit_read_ids ∪ injection_ids| (search exposures excluded)
    pub total_served: u64,

    // Unchanged fields below
    pub cross_session_count: u64,
    pub by_category: HashMap<String, u64>,  // search-exposure sourced; relabeled in render
    pub category_gaps: Vec<String>,
    pub total_stored: u64,
    // cross_feature_reuse, intra_cycle_reuse, top_cross_feature_entries — unchanged
}
```

### Explicit Read Filter Predicate

An `ObservationRecord` from `attributed` qualifies as an explicit read when ALL hold:
1. `event_type == EventType::PreToolUse`
2. `normalize_tool_name(tool.as_deref().unwrap_or(""))` is `"context_get"` or `"context_lookup"`
3. `record.input` is `Some(Value::String(_))` or `Some(Value::Object(_))`
4. After two-branch parse (String branch: `serde_json::from_str(s).ok()`; Object branch: use as-is), the resulting object has field `id`
5. `obj["id"].as_u64().or_else(|| obj["id"].as_str().and_then(|s| s.parse().ok()))` returns `Some(n)`

Condition 5 is the natural exclusion predicate for filter-based `context_lookup` (no `id` field fails both paths; no special casing required).

---

## Function Signatures

### New: `extract_explicit_read_ids` — `knowledge_reuse.rs`

```rust
pub(crate) fn extract_explicit_read_ids(
    attributed: &[ObservationRecord],
) -> HashSet<u64>
```

Pure function over in-memory slice; no DB access; no async. Calls `unimatrix_observe::normalize_tool_name` before any tool name comparison. Handles `Option<String>` tool field via `tool.as_deref().unwrap_or("")`.

### Extended: `compute_knowledge_reuse` — `knowledge_reuse.rs`

Two new parameters appended to existing signature:
```rust
explicit_read_ids: &HashSet<u64>,
explicit_read_meta: &HashMap<u64, EntryMeta>,
```

New computations:
- `explicit_read_count = explicit_read_ids.len() as u64`
- `explicit_read_by_category`: tally `EntryMeta.category` strings from `explicit_read_meta`
- `total_served = (explicit_read_ids | &injection_entry_ids).len() as u64` (set union, both `HashSet<u64>`)
- Early-return guard: `total_served == 0 && search_exposure_count == 0`

### Extended: `compute_knowledge_reuse_for_sessions` — `tools.rs`

```rust
async fn compute_knowledge_reuse_for_sessions(
    store: &Arc<SqlxStore>,
    session_records: &[SessionRecord],
    feature_cycle: &FeatureCycle,
    attributed: &[ObservationRecord],   // new parameter
) -> FeatureKnowledgeReuse
```

New steps within this function:
1. `let explicit_ids = extract_explicit_read_ids(attributed);`
2. Apply `EXPLICIT_READ_META_CAP = 500`: if `explicit_ids.len() > 500`, warn and truncate for the lookup only
3. `let explicit_meta_map = batch_entry_meta_lookup(store, lookup_ids).await;`
4. Pass `&explicit_ids` and `&explicit_meta_map` into `compute_knowledge_reuse`

### `EXPLICIT_READ_META_CAP` constant — `tools.rs`

```rust
const EXPLICIT_READ_META_CAP: usize = 500;
```

Placed near `compute_knowledge_reuse_for_sessions`. Cap applies only to the category join input; `explicit_read_count` is always computed from the full uncapped `HashSet`.

### `SUMMARY_SCHEMA_VERSION` — `cycle_review_index.rs`

```rust
pub const SUMMARY_SCHEMA_VERSION: u32 = 3;  // was 2 after crt-047
```

Advisory message text must be specific: "schema_version 2 predates the explicit read signal and total_served redefinition (search exposures no longer contribute to total_served); use force=true to recompute".

---

## Constraints

- **No schema migration**: `observations` table already has `input` (JSON) and `phase`. No new columns, tables, or migration steps.
- **No new crate dependencies**: `ObservationRecord` is in `unimatrix-core` (already a dependency of `unimatrix-server`); `FeatureKnowledgeReuse` is in `unimatrix-observe`. No new inter-crate edges.
- **Serde alias chain**: `search_exposure_count` must carry BOTH `"delivery_count"` AND `"tier1_reuse_count"` as stacked `#[serde(alias)]` lines. Dropping either alias silently corrupts metrics for pre-existing stored rows (no error, no diagnostic).
- **`SUMMARY_SCHEMA_VERSION` bump is mandatory**: Pattern #4178 (bump policy): adding `explicit_read_count`, adding `explicit_read_by_category`, and redefining `total_served` all qualify. Skipping causes stale cached records to be returned silently.
- **`batch_entry_meta_lookup` batching**: Single batched IN-clause call, chunked at 100 IDs per ADR-003 (col-026). N+1 per-ID queries are not acceptable (C-03, NFR-03).
- **`attributed` must be unfiltered**: The slice passed into `compute_knowledge_reuse_for_sessions` must be the full unfiltered slice from step 12 of `context_cycle_review`. Any upstream truncation silently undercounts `explicit_read_count` (C-04).
- **`normalize_tool_name` is mandatory**: Hook-sourced `PreToolUse` events carry `mcp__unimatrix__` prefix. Bare string comparison produces `explicit_read_count = 0` for all production cycles with no diagnostic (AC-06 [GATE]).
- **`total_served` must NOT include search exposures**: Including them conflates consumption with delivery and inflates the metric by up to an order of magnitude (C-06, AC-14 [GATE]).
- **`cross_session_count` extension deferred**: Do not implement (C-07).
- **Phase-stratified breakdowns deferred**: `phase` is available on `ObservationRecord` but must not be used in this feature (C-08). Group 10 owns phase aggregation.
- **Early-return guard must be `total_served == 0 && search_exposure_count == 0`**: The old three-condition form (`search_exposure_count == 0 && explicit_read_count == 0 && injection_count == 0`) must NOT be present anywhere. An injection-only cycle has `total_served > 0` and must not be suppressed (AC-17 [GATE]).

---

## Dependencies

| Dependency | Kind | Changes |
|-----------|------|---------|
| `unimatrix-observe/src/types.rs` | Internal | `FeatureKnowledgeReuse` — modified |
| `unimatrix-server/src/mcp/knowledge_reuse.rs` | Internal | `compute_knowledge_reuse`, `extract_explicit_read_ids` — modified / added |
| `unimatrix-server/src/tools.rs` | Internal | `compute_knowledge_reuse_for_sessions`, call site at step 13-14 — modified |
| `unimatrix-server/src/mcp/response/retrospective.rs` | Internal | `render_knowledge_reuse` — guard and labels modified |
| `unimatrix-store/src/cycle_review_index.rs` | Internal | `SUMMARY_SCHEMA_VERSION` bumped; advisory message updated |
| `unimatrix-core/src/observation.rs` | Internal | `ObservationRecord` — read-only, no changes |
| `unimatrix_observe::normalize_tool_name` | Internal | Called in `extract_explicit_read_ids` — no changes to function |
| `batch_entry_meta_lookup` (tools.rs line 3143) | Internal | Called twice in `compute_knowledge_reuse_for_sessions` — second call is new (explicit read IDs) |
| No new external crates | — | No new crate edges introduced |

---

## NOT in Scope

- Removing `search_exposure_count`, `query_log` loading, or `injection_log` loading — both remain as separate sub-metrics
- Adding `context_get` / `context_lookup` writes to `query_log` — observation recording is already correct and unchanged
- Deduplicating explicit reads against search exposures — the two metrics are independently meaningful
- Extending `cross_session_count` to cover explicit reads
- Phase-stratified explicit read breakdowns (Group 10 / ASS-040 scope)
- Adding `phase` breakdowns to any field in `FeatureKnowledgeReuse`
- Extending `PhaseFreqTable` or the `query_log`-based phase frequency pipeline
- New DB tables, columns, or schema migrations
- Phase-conditioned category affinity computation (ASS-040 Group 10 — depends on this feature, ships after)

---

## Alignment Status

**Overall: PASS** — Vision alignment PASS (0 variances). 1 WARN (benign). 2 VARIANCEs identified in the alignment report and resolved before delivery via authoritative values from SCOPE.md and SPECIFICATION.md.

**WARN** (benign, no action required): ADR-004 (500-ID cardinality cap on `explicit_read_by_category` batch lookup) is present in the architecture and risk strategy but not stated in SCOPE.md. It directly resolves SR-03 from the risk assessment. No user-visible behavior change at realistic cycle sizes. Documented in ADR-004.

**VARIANCE 1 resolved**: ARCHITECTURE.md Component 2 (knowledge_reuse.rs, line 48) still listed the old early-return guard (`search_exposure_count == 0 && explicit_read_count == 0 && injection_count == 0`). The authoritative condition is from SCOPE.md AC-17 [GATE] and SPECIFICATION.md FR-08: `total_served == 0 && search_exposure_count == 0`. Delivery engineers must implement the corrected guard. The stale text in ARCHITECTURE.md Component 2 must be treated as superseded by SCOPE.md AC-17.

**VARIANCE 2 resolved**: RISK-TEST-STRATEGY.md R-05 scenario 2 and R-06 scenario 3 described the old guard condition; the Coverage Summary gate list omitted AC-16 [GATE] and AC-17 [GATE]. The authoritative gate list is: **AC-02, AC-06, AC-13, AC-14, AC-15, AC-16, AC-17**. Test construction must follow SCOPE.md and SPECIFICATION.md, not the stale scenarios in the risk strategy document.

**Gate ACs (delivery merge blocked if failing)**: AC-02, AC-06, AC-13, AC-14, AC-15, AC-16, AC-17.
