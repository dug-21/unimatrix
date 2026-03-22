# crt-026: WA-2 Session Context Enrichment — Implementation Brief

GH Issue: #341

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-026/SCOPE.md |
| Scope Risk Assessment | product/features/crt-026/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-026/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-026/specification/SPECIFICATION.md |
| Risk & Test Strategy | product/features/crt-026/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-026/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/crt-026/architecture/ADR-001-fused-score-inputs-integration.md |
| ADR-002 | product/features/crt-026/architecture/ADR-002-pre-resolve-histogram-in-handler.md |
| ADR-003 | product/features/crt-026/architecture/ADR-003-w-phase-explicit-zero-placeholder.md |
| ADR-004 | product/features/crt-026/architecture/ADR-004-no-weight-rebalancing.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| SessionState + SessionRegistry methods | pseudocode/session.md | test-plan/session.md |
| context_store handler — histogram recording | pseudocode/store-handler.md | test-plan/store-handler.md |
| ServiceSearchParams — data carrier fields | pseudocode/search-params.md | test-plan/search-params.md |
| context_search handler — pre-resolution | pseudocode/search-handler.md | test-plan/search-handler.md |
| FusedScoreInputs / FusionWeights / compute_fused_score | pseudocode/fused-score.md | test-plan/fused-score.md |
| InferenceConfig — new weight fields | pseudocode/config.md | test-plan/config.md |
| UDS handle_context_search + handle_compact_payload | pseudocode/uds.md | test-plan/uds.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Stage 3a complete. All pseudocode and test-plan files produced.

---

## Goal

Add an implicit, zero-configuration session context signal to `context_search` by accumulating
a per-session category histogram in `SessionState` and feeding it into `compute_fused_score` as
a seventh dimension (`phase_histogram_norm`). Results whose category matches what the session has
been storing receive a small additive boost; cold-start sessions (no prior stores) produce
bit-for-bit identical results to the pre-crt-026 pipeline. The histogram summary is also surfaced
in the UDS `CompactPayload` synthesis output to inform agents at compaction time.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Boost integration point: inside `compute_fused_score` vs. post-pipeline additive | Integrate as first-class dimension inside `compute_fused_score` so `status_penalty` applies uniformly and W3-1 sees a named, learnable field | ADR-001, OQ-01 | architecture/ADR-001-fused-score-inputs-integration.md |
| Session registry access in `SearchService`: `Arc<SessionRegistry>` on service vs. pre-resolve in handler | Pre-resolve histogram in tool handler before `await`; pass via `ServiceSearchParams.category_histogram`; `SearchService` holds no session registry reference | ADR-002, OQ-02 | architecture/ADR-002-pre-resolve-histogram-in-handler.md |
| Explicit phase term (`w_phase_explicit`): ship behavior or defer | Ship as `w_phase_explicit=0.0` placeholder in `InferenceConfig` + `FusionWeights` + `FusedScoreInputs.phase_explicit_norm=0.0`; no `phase_category_weight` mapping; deferred to W3-1 | ADR-003, OQ-03 | architecture/ADR-003-w-phase-explicit-zero-placeholder.md |
| Weight sum: `w_phase_histogram=0.02` carries full session signal budget | No rebalancing; existing six-weight defaults unchanged; `w_phase_histogram=0.02` (ASS-028 calibrated value) is additive; sum goes 0.95→0.97; `InferenceConfig::validate()` six-field sum check not modified | ADR-004, OQ-04 | architecture/ADR-004-no-weight-rebalancing.md |
| `status_penalty` application order | Confirmed: `final_score = compute_fused_score(...) * status_penalty` — boost is inside fused score before penalty | ARCHITECTURE.md OQ-D | — |
| `InferenceConfig::validate()` with sum=0.97 | Confirmed: existing check tests only the six original fields (sum stays 0.95); 0.97 total passes; no existing test asserts `sum==0.95` against defaults | ARCHITECTURE.md OQ-A | — |
| UDS `handle_context_search` session_id source | `HookRequest::ContextSearch.session_id` field; `sanitize_session_id` already called at lines 796-803; histogram pre-resolution placed after sanitize check | ARCHITECTURE.md OQ-B | — |
| WA-4a forward-compatibility with pre-resolution pattern | Pre-resolution is correct for crt-026; WA-4a will likely need `Arc<SessionRegistry>` on `SearchService` and must supersede ADR-002 at that time | ARCHITECTURE.md OQ-C | — |

---

## Files to Create or Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/infra/session.rs` | Modify | Add `category_counts: HashMap<String, u32>` to `SessionState`; add `record_category_store` and `get_category_histogram` methods to `SessionRegistry` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Call `record_category_store` after non-duplicate store success (Component 2); pre-resolve histogram + thread `session_id` and `category_histogram` into `ServiceSearchParams` in `context_search` handler (Component 4) |
| `crates/unimatrix-server/src/services/search.rs` | Modify | Add `session_id` and `category_histogram` fields to `ServiceSearchParams`; add `phase_histogram_norm` and `phase_explicit_norm` to `FusedScoreInputs`; add `w_phase_histogram` and `w_phase_explicit` to `FusionWeights`; extend `compute_fused_score` with both new terms; update `FusionWeights::effective()` NLI-absent pass-through; remove WA-2 extension stub comments |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Add `w_phase_explicit: f64` (default `0.0`) and `w_phase_histogram: f64` (default `0.02`) to `InferenceConfig`; add per-field `[0.0, 1.0]` range checks in `validate()`; update `Default::default()` struct literal |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | Add histogram pre-resolution block after `sanitize_session_id` in `handle_context_search`; pass `session_id` and `category_histogram` into `ServiceSearchParams`; add `get_category_histogram` call in `handle_compact_payload` and append histogram summary block to `format_compaction_payload` when non-empty |

---

## Data Structures

### `SessionState` (extended)

```rust
pub struct SessionState {
    // ... existing fields (current_phase, etc.) unchanged ...
    pub category_counts: HashMap<String, u32>,  // NEW: per-session category histogram
}
```

Initialized to `HashMap::new()` in `register_session`. In-memory only; never persisted.

### `ServiceSearchParams` (extended)

```rust
pub struct ServiceSearchParams {
    // ... existing fields unchanged ...
    pub session_id: Option<String>,                       // NEW: for logging/tracing
    pub category_histogram: Option<HashMap<String, u32>>, // NEW: pre-resolved histogram
}
```

`category_histogram` is `None` when there is no session, the session is unregistered, or the
histogram is empty (handler maps `is_empty()` result to `None`).

### `FusedScoreInputs` (extended)

```rust
pub struct FusedScoreInputs {
    // ... existing six fields unchanged ...
    pub phase_histogram_norm: f64, // NEW: p(entry.category) from session histogram, [0.0, 1.0]
    pub phase_explicit_norm: f64,  // NEW: W3-1 placeholder, always 0.0 in crt-026 (ADR-003)
}
```

### `FusionWeights` (extended)

```rust
pub struct FusionWeights {
    // ... existing six weight fields unchanged ...
    pub w_phase_histogram: f64, // NEW: default 0.02 (ASS-028 calibrated value, full session signal budget)
    pub w_phase_explicit: f64,  // NEW: default 0.0 (W3-1 placeholder, ADR-003)
}
```

The invariant doc-comment on `FusionWeights` is updated from `sum of six <= 1.0` to:
`sum of six core terms <= 1.0; w_phase_histogram and w_phase_explicit are additive terms
excluded from this constraint`.

`FusionWeights::effective()` NLI-absent re-normalization denominator must enumerate only the five
core terms (`w_sim + w_conf + w_coac + w_util + w_prov`); both phase fields are passed through
unchanged in both NLI-active and NLI-absent paths.

### `InferenceConfig` (extended)

```rust
#[serde(default = "default_w_phase_explicit")]
pub w_phase_explicit: f64,   // default 0.0

#[serde(default = "default_w_phase_histogram")]
pub w_phase_histogram: f64,  // default 0.02
```

---

## Function Signatures

### New methods on `SessionRegistry` (`infra/session.rs`)

```rust
pub fn record_category_store(&self, session_id: &str, category: &str)
// Increments category_counts[category] by 1 for the registered session.
// Silent no-op for unregistered sessions. Synchronous lock hold, no await.

pub fn get_category_histogram(&self, session_id: &str) -> HashMap<String, u32>
// Returns a clone of category_counts, or an empty HashMap if not registered.
// Sole read path for the histogram.
```

### Extended function `compute_fused_score` (`services/search.rs`)

The final formula with all eight terms:

```
score = w_nli * nli
      + w_sim * sim
      + w_conf * conf
      + w_coac * coac
      + w_util * util
      + w_prov * prov
      + w_phase_histogram * phase_histogram_norm   // NEW: histogram affinity boost
      + w_phase_explicit  * phase_explicit_norm    // NEW: always 0.0 in crt-026 (ADR-003)

final_score = score * status_penalty               // penalty applied after fused score
```

Max histogram boost with defaults: `0.02 * 1.0 = 0.02` (at p=1.0 concentration). Within the
0.05 WA-0 headroom. When W3-1 enables `w_phase_explicit`, W3-1 will learn the appropriate
split between the two terms.

### Scoring loop histogram resolution (`services/search.rs`)

```rust
let category_histogram = params.category_histogram.as_ref();
let total: u32 = category_histogram
    .map(|h| h.values().sum())
    .unwrap_or(0);

// Per-candidate:
let phase_histogram_norm = if total > 0 {
    category_histogram
        .and_then(|h| h.get(&entry.category))
        .copied()
        .unwrap_or(0) as f64 / total as f64
} else {
    0.0
};
```

### Histogram summary block (`uds/listener.rs`)

Format when non-empty (top-5 by count descending, only counts > 0):
```
Recent session activity: decision × 3, pattern × 2
```
Omit entirely when histogram is empty. Must fit within `MAX_INJECTION_BYTES` budget (< 100 bytes
for typical sessions with a vocabulary of < 20 categories).

---

## Constraints

1. **No schema changes.** `category_counts` is in-memory per-session state only. No new tables,
   no migration, no schema version bump.
2. **No new crates.** All changes in `crates/unimatrix-server`. Specifically:
   `infra/session.rs`, `infra/config.rs`, `services/search.rs`, `mcp/tools.rs`, `uds/listener.rs`.
3. **No weight rebalancing.** The six existing `InferenceConfig` weight defaults are unchanged.
   `w_phase_histogram=0.02` is additive (full session signal budget, ADR-004). Sum goes 0.95→0.97.
   `InferenceConfig::validate()` six-field sum check is NOT modified. Per-field `[0.0, 1.0]` range
   checks ARE added for both new fields.
4. **`FusionWeights::effective()` NLI-absent path** must NOT include `w_phase_histogram` or
   `w_phase_explicit` in the re-normalization denominator. Both are passed through unchanged.
5. **Cold-start safety.** Empty histogram → `category_histogram = None` → all
   `phase_histogram_norm = 0.0` → `compute_fused_score` output is bit-for-bit identical to
   pre-crt-026 results. No behavioral regression for sessions without histogram data.
6. **Duplicate-store guard placement.** `record_category_store` is called AFTER the duplicate
   check (`insert_result.duplicate_of.is_some()`). Duplicate stores must never increment the
   histogram.
7. **Pre-resolution before `await`.** The `get_category_histogram` call in both `context_search`
   MCP handler and `handle_context_search` UDS handler must occur before any `await` point,
   following the crt-025 SR-07 snapshot pattern.
8. **UDS sanitization ordering.** In `handle_context_search`, `sanitize_session_id` is already
   applied at lines 796-803. Histogram pre-resolution must be placed AFTER the sanitize check.
9. **`phase_explicit_norm` always `0.0`.** The ADR-003 placeholder field is hardcoded to `0.0`
   at the call site. A comment citing ADR-003 must accompany this assignment to prevent future
   removal as dead code.
10. **WA-2 extension stubs resolved.** All `// WA-2 extension:` comments at lines 55, 89, 179 of
    `search.rs` are replaced with the implemented field declarations and doc-comments. No stub
    comment may remain after implementation.
11. **Boost bounded.** Max histogram boost = `0.02` with defaults (p=1.0 concentration). Never
    pushes a weak-similarity entry above a high-NLI entry given current calibrations (NLI dominant at 0.35).
12. **Hook timeout budget.** UDS histogram summary is string formatting on pre-resolved in-memory
    data. No I/O, no SQL. Negligible latency; must not approach the 40ms `HOOK_TIMEOUT` budget.

---

## Dependencies

| Dependency | Type | Status | Notes |
|------------|------|--------|-------|
| crt-025 (WA-1, GH #330) | Upstream feature | Complete | Provides `SessionState.current_phase`, `set_current_phase()`, SR-07 snapshot pattern |
| crt-024 (WA-0) | Upstream feature | Complete | Provides `FusedScoreInputs`, `FusionWeights`, `compute_fused_score`, WA-2 extension stubs |
| `rusqlite` / `unimatrix-store` | Storage crate | No changes required | |
| `rmcp 0.16.0` | MCP server | No changes required | |
| `SessionRegistry` / `SessionState` | `infra/session.rs` | Extended by this feature | |
| `ServiceSearchParams` | `services/search.rs` | Extended by this feature | |
| `FusedScoreInputs`, `FusionWeights`, `compute_fused_score` | `services/search.rs` | Extended by this feature | WA-2 stubs at lines 55, 89, 179 are the integration contract |
| `InferenceConfig` | `infra/config.rs` | Extended by this feature | |
| `format_compaction_payload` / `handle_compact_payload` | `uds/listener.rs` | Extended by this feature | |
| W3-1 (GNN relevance function) | Downstream consumer | Not yet scheduled | Uses `w_phase_histogram=0.02` as cold-start seed (ASS-028 calibrated); uses `phase_explicit_norm` field once ready |
| WA-4a (proactive injection) | Downstream feature | Not yet scheduled | Will likely need to supersede ADR-002 (`Arc<SessionRegistry>` on `SearchService`) |

---

## NOT in Scope

- Markov model or transition prediction for category sequences
- Phase-annotated co-access edges (W1-5)
- GNN training data production or label generation
- Changes to `context_briefing` behavior
- Changes to `context_lookup` (deterministic, filter-based)
- Histogram decay or time-weighting of older session stores
- Explicit phase boost behavior (`w_phase_explicit > 0.0`, `phase_category_weight` mapping) — deferred to W3-1
- Changes to `context_cycle` tool or phase machinery (crt-025, already complete)
- Changes to WA-3 (MissedRetrieval) or WA-4 (Proactive Delivery)
- Any new database tables or schema migrations
- Any new workspace crates

---

## Non-Negotiable Test Requirements (Gate Blockers)

Seven tests must pass to clear Gate 3c. All map to acceptance criteria and risk entries.

| # | Test Name | Covers | Risk |
|---|-----------|--------|------|
| 1 | `test_histogram_boost_score_delta_at_p1_equals_weight` | AC-12: score delta ≥ 0.02 with p=1.0 concentration; must assert a numerical floor, not just "ranks higher" | R-01 |
| 2 | `test_duplicate_store_does_not_increment_histogram` | AC-02: store same entry twice; assert histogram count = 1 after second store | R-03 |
| 3 | `test_cold_start_search_produces_identical_scores` | AC-08: empty histogram → all `phase_histogram_norm = 0.0` → scores bit-for-bit identical to pre-crt-026 | R-02 |
| 4 | `test_record_category_store_unregistered_session_is_noop` | AC-03: call with unknown session_id; no panic, no state change | R-04 |
| 5 | `test_compact_payload_histogram_block_present_and_absent` | AC-11: non-empty histogram → block present; empty histogram → block absent | R-10 |
| 6 | `test_absent_category_phase_histogram_norm_is_zero` | AC-13: category not in histogram → `phase_histogram_norm = 0.0` | R-01, R-13 |
| 7 | `test_fusion_weights_effective_nli_absent_excludes_phase_from_denominator` | `FusionWeights::effective(false)` passes `w_phase_histogram` through unchanged; re-normalization denominator is five terms only | R-06 |

Additional high-priority tests (not gate blockers but required for full coverage):

- `test_60_percent_concentration_score_delta` — p=0.6 produces delta of exactly `0.02 * 0.6 = 0.012` (R-01 scenario 2)
- `test_status_penalty_applied_after_histogram_boost` — `(fused + boost) * penalty` ordering confirmed (R-08, AC-10)
- `test_uds_search_path_histogram_pre_resolution` — UDS `handle_context_search` populates `category_histogram` from session (R-05, FR-07)
- `test_config_validation_rejects_out_of_range_phase_weights` — `w_phase_histogram > 1.0` fails validation (R-11)
- `test_phase_explicit_norm_placeholder_fields_present` — `FusedScoreInputs` and `FusionWeights` have the placeholder fields; `InferenceConfig::default()` returns `w_phase_explicit=0.0, w_phase_histogram=0.02` (R-07, AC-09)

---

## Alignment Status

Source: `product/features/crt-026/ALIGNMENT-REPORT.md` (reviewed 2026-03-22)

| Check | Status |
|-------|--------|
| Vision Alignment | WARN (two variances — see below) |
| Milestone Fit | PASS |
| Scope Gaps | PASS |
| Scope Additions | PASS |
| Architecture Consistency | WARN (V-2 — see below) |
| Risk Completeness | PASS |

### V-1 (WARN): Product vision WA-2 pipeline diagram is inaccurate after ADR-001

The product vision (PRODUCT-VISION.md line 230) describes the WA-2 pipeline as:
`HNSW(k=20) → NLI re-rank → co-access boost → category affinity boost → top-k`
placing the affinity boost as a post-pipeline step. ADR-001 integrates the boost inside
`compute_fused_score` as a first-class dimension — architecturally correct for W3-1 compatibility
and safer for deprecated entry penalty ordering (`(base + boost) * penalty` vs. `base * penalty + boost`).

**Post-delivery action required**: Update `PRODUCT-VISION.md` WA-2 pipeline diagram to:
```
HNSW(k=20) → NLI re-rank → compute_fused_score (with histogram term) → status_penalty → top-k
```
and update the WA-0 headroom note to clarify the 0.05 headroom is consumed inside
`compute_fused_score`. This is a documentation correction, not a code change. The architectural
decision is accepted.

### V-2 (WARN): `phase_explicit_norm = 0.0` is a permanent dead-code field until W3-1

`FusedScoreInputs.phase_explicit_norm` and `FusionWeights.w_phase_explicit` are always `0.0`
in crt-026 — a no-op term inside `compute_fused_score`. This is the ADR-003 decision: deferred
to W3-1, which will populate the field using a learned phase-to-category model. The placeholder
is accepted on condition that a comment citing ADR-003 is present at the call site to prevent
future removal as dead code (RISK-TEST-STRATEGY.md R-07). The W3-1 roadmap (Wave 3) is the
intended resolution. No blocking action required for crt-026 delivery.
