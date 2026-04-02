# crt-040 Implementation Brief — Cosine Supports Edge Detection

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-040/SCOPE.md |
| Architecture | product/features/crt-040/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-040/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-040/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-040/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| unimatrix-store / read.rs | pseudocode/store-constant.md | test-plan/store-constant.md |
| unimatrix-server / nli_detection.rs | pseudocode/write-graph-edge.md | test-plan/write-graph-edge.md |
| unimatrix-server / infra/config.rs | pseudocode/inference-config.md | test-plan/inference-config.md |
| unimatrix-server / nli_detection_tick.rs | pseudocode/path-c-loop.md | test-plan/path-c-loop.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Restore `Supports` edge production to the Unimatrix knowledge graph by implementing a
pure-cosine detection path (Path C) inside `run_graph_inference_tick`. The NLI post-store
path was deleted in crt-038, leaving zero `Supports` edges in production; ASS-035
validated that cosine similarity at threshold >= 0.65 correctly identifies true `Supports`
pairs with zero false positives on the production corpus. Path C reuses the Phase 4
candidate set already computed by the tick — no new HNSW scan — and tags every written
edge `source = 'cosine_supports'` so GNN feature construction and PPR traversal can
distinguish the signal source.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Edge writer generalization strategy | Add `write_graph_edge(source: &str, ...)` as a sibling function in `nli_detection.rs`. `write_nli_edge` is NOT modified — it retains its hardcoded `source='nli'`. Path C calls `write_graph_edge` directly. Parameterizing `write_nli_edge` rejected: would require updating all callers and creates mis-tagging risk. | Pattern #4025 | product/features/crt-040/architecture/ADR-001-edge-writer-generalization.md |
| InferenceConfig dual-site default requirement | `supports_cosine_threshold` must be set in BOTH `#[serde(default = "default_supports_cosine_threshold")]` AND `impl Default for InferenceConfig`. The impl Default literal must call the backing function: `supports_cosine_threshold: default_supports_cosine_threshold()`. This eliminates single-source-of-truth divergence (crt-038 gate-3b root cause). | Pattern #3817, Lesson #4014 | product/features/crt-040/architecture/ADR-002-impl-default-dual-site-requirement.md |
| Path C placement in tick | Path C runs after Path A (Informs write loop + observability log) and before the Path B entry gate (`get_provider()` check). This maintains the structural-paths-before-model-paths ordering and ensures Path C is always unconditional. | ADR-003 | product/features/crt-040/architecture/ADR-003-path-c-placement-in-tick.md |
| Cosine Supports per-tick budget | `MAX_COSINE_SUPPORTS_PER_TICK = 50` module-level constant. Independent of `MAX_INFORMS_PER_TICK` (25) and `max_graph_inference_per_tick` (Path B budget). Not config-promoted — constant follows the `MAX_INFORMS_PER_TICK` pattern. TODO comment at constant site must note config-promotion as a future extension point. | ADR-004 | product/features/crt-040/architecture/ADR-004-cosine-supports-budget-constant.md |
| Category filter implementation | Build `HashMap<u64, String>` (entry_id to category) from `all_active` after Phase 2, before the Path C loop. Per-pair DB lookup is PROHIBITED. See WARN-01 and R-01 mandate below. | RISK-TEST-STRATEGY R-01, ALIGNMENT-REPORT WARN-01 | — |
| `nli_post_store_k` removal | Dead field (consumer deleted in crt-038) removed in the same delivery. Serde ignores unknown fields — existing config files silently drop the value. Verified by AC-18. | SPECIFICATION.md FR-09 | — |
| `weight = cosine` for Path C edges | No PPR weight multiplier. The cosine value at >= 0.65 serves as the PPR weight directly. No new config knob — no validated use yet. | SCOPE.md Resolved Decision 3 | — |
| Edge metadata | `{"cosine": <f32>}` — short, consistent with cosine values elsewhere. | SCOPE.md Resolved Decision 4 | — |
| `inferred_edge_count` left unchanged | Backward compat: `inferred_edge_count` continues to count only `source='nli'` edges. Semantic staleness deferred to follow-up issue. Eval gate uses `supports_edge_count` which is source-agnostic. | SCOPE.md Resolved Decision 5 | — |
| Config merge function site | The merge function in `config.rs` for `supports_cosine_threshold` must be updated following the `nli_informs_cosine_floor` merge pattern (f32 epsilon comparison). Grep the merge function body for `nli_informs_cosine_floor` to locate the correct site. Delivery must add `supports_cosine_threshold` at that site. Missing this causes project-level config overrides to be silently ignored (R-13). | RISK-TEST-STRATEGY R-13, SPECIFICATION.md FR-08 | — |

---

## Files to Create or Modify

| File | Change | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/read.rs` | Modify | Add `pub const EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports";` following `EDGE_SOURCE_NLI` and `EDGE_SOURCE_CO_ACCESS` pattern |
| `crates/unimatrix-store/src/lib.rs` | Modify | Re-export `EDGE_SOURCE_COSINE_SUPPORTS` alongside the existing two source constants |
| `crates/unimatrix-server/src/services/nli_detection.rs` | Modify | Add `pub(crate) async fn write_graph_edge(...)` sibling function; `write_nli_edge` is NOT changed |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Add `supports_cosine_threshold: f32` field (dual-site: serde backing fn + impl Default); remove `nli_post_store_k` field and all 6 associated sites; update config merge function |
| `crates/unimatrix-server/src/services/nli_detection_tick.rs` | Modify | Add `MAX_COSINE_SUPPORTS_PER_TICK = 50` constant; add Path C write loop after Path A and before Path B gate; add unconditional Path C observability log; build category HashMap before loop |

---

## Data Structures

### InferenceConfig (modified)
```rust
// New field (dual-site: serde default + impl Default)
#[serde(default = "default_supports_cosine_threshold")]
pub supports_cosine_threshold: f32,  // default 0.65, range (0.0, 1.0) exclusive

// Removed field (dead since crt-038)
// nli_post_store_k: usize  -- DELETE this field and all associated sites
```

### graph_edges (no schema change)
```
source_id:      u64
target_id:      u64  (canonical: source_id < target_id via Phase 4 normalization)
relation_type:  "Supports"
weight:         f32  (= cosine value, no multiplier)
source:         "cosine_supports"  (EDGE_SOURCE_COSINE_SUPPORTS)
created_by:     "cosine_supports"
metadata:       '{"cosine": <f32>}'
bootstrap_only: 0
```

### Category lookup HashMap (Path C pre-requisite)
```rust
// Built once from all_active after Phase 2, before the Path C loop
// Key: entry_id (u64), Value: category (String)
let category_map: HashMap<u64, String> = all_active
    .iter()
    .map(|e| (e.id, e.category.clone()))
    .collect();
```

### candidate_pairs (Phase 4 output, consumed by Path C)
```rust
Vec<(u64, u64, f32)>  // (source_id, target_id, cosine) — canonical (lo, hi) form
```

---

## Function Signatures

### New: `write_graph_edge` (nli_detection.rs)
```rust
pub(crate) async fn write_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f32,
    created_at: u64,
    source: &str,
    metadata: &str,
) -> bool
// Issues INSERT OR IGNORE into graph_edges.
// On SQL error: logs warn!, returns false.
// On UNIQUE conflict (INSERT OR IGNORE): returns false. NOT an error — do not warn.
// On success: returns true.
// created_by is set to the same value as source.
```

### New: `default_supports_cosine_threshold` (config.rs)
```rust
fn default_supports_cosine_threshold() -> f32 {
    0.65
}
```

### New constants
```rust
// unimatrix-store/src/read.rs
pub const EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports";

// unimatrix-server/src/services/nli_detection_tick.rs
const MAX_COSINE_SUPPORTS_PER_TICK: usize = 50;
// TODO: config-promote to InferenceConfig.max_cosine_supports_per_tick if operators
// require runtime tuning. Not speculated in crt-040.
```

---

## Path C Loop Pseudocode

```
=== PATH C: Cosine Supports write loop (NEW) ===

// Pre-build category map once (MANDATORY — per-pair DB lookup prohibited)
let category_map: HashMap<u64, String> = build_from(all_active);

let mut cosine_supports_written = 0usize;
let mut cosine_supports_candidates = 0usize;

for (src_id, tgt_id, cosine) in &candidate_pairs {
    // Guard: finite cosine (NaN/Inf from HNSW boundary)
    if !cosine.is_finite() {
        warn!("Path C: non-finite cosine for pair ({}, {})", src_id, tgt_id);
        continue;
    }

    // Gate 1: cosine threshold
    if cosine < config.supports_cosine_threshold { continue; }

    cosine_supports_candidates += 1;

    // Gate 2: budget cap
    if cosine_supports_written >= MAX_COSINE_SUPPORTS_PER_TICK { break; }

    // Gate 3: category pair filter (O(1) via HashMap — no DB lookup)
    let Some(src_cat) = category_map.get(src_id) else {
        warn!("Path C: entry {} not in all_active (deprecated mid-tick?)", src_id);
        continue;
    };
    let Some(tgt_cat) = category_map.get(tgt_id) else {
        warn!("Path C: entry {} not in all_active (deprecated mid-tick?)", tgt_id);
        continue;
    };
    if !category_pair_allowed(src_cat, tgt_cat, &config.informs_category_pairs) {
        continue;
    }

    // Gate 4: pre-filter (INSERT OR IGNORE is the authoritative backstop)
    let canonical = (src_id.min(tgt_id), src_id.max(tgt_id));
    if existing_supports_pairs.contains(&canonical) { continue; }

    // Write edge
    let wrote = write_graph_edge(
        store, *src_id, *tgt_id, "Supports", *cosine,
        current_timestamp_secs(), EDGE_SOURCE_COSINE_SUPPORTS,
        &format!(r#"{{"cosine":{}}}"#, cosine),
    ).await;

    // Budget counter: only increment on true return
    if wrote {
        cosine_supports_written += 1;
    }
    // false return (UNIQUE conflict) is NOT an error — INSERT OR IGNORE is expected
}

// Unconditional observability log — fires even when both counts are zero
debug!(
    cosine_supports_candidates,
    cosine_supports_edges_written = cosine_supports_written,
    "Path C: cosine Supports tick complete"
);
```

Note: Budget gate (Gate 2) is placed before category lookup to avoid HashMap lookups for
candidates that would be discarded by the budget cap. The `break` on budget exhaustion
terminates the loop — Phase 4's priority ordering (cross-category pairs first, then
by similarity descending) means the highest-value candidates are consumed first.

---

## Critical Implementation Mandates (from Vision Alignment Variances)

### WARN-01: HashMap Category Pre-Build Is MANDATORY (R-01)

`candidate_pairs` is `Vec<(u64, u64, f32)>` — it contains no category data. Path C needs
`source_category` and `target_category` for each pair to apply the `informs_category_pairs`
filter.

**Mandate**: Build `HashMap<u64, String>` (entry_id to category) from `all_active` once,
after Phase 2 completes and before the Path C loop begins. Per-pair DB lookup in the Path C
loop is PROHIBITED — it violates the hot-path performance contract and is architecturally
equivalent to the no-new-scan constraint.

No AC in the specification explicitly mandates the HashMap over a per-pair DB lookup. This
brief fills that gap. The implementation MUST use the HashMap. A delivery agent must not
satisfy AC-03 via SQL round-trips inside the Path C loop.

### WARN-02: Path C Observability Log Is MANDATORY (R-06, ADR-003)

Path C must emit a structured log unconditionally after the write loop, even when
`candidate_pairs` is empty or all candidates are filtered. Field names:
- `cosine_supports_candidates` — count of pairs that passed cosine threshold (before
  category filter and budget check)
- `cosine_supports_edges_written` — count of edges successfully written

This fires as a `debug!` event placed OUTSIDE the write loop. It is not optional — it is
the mechanism by which operators distinguish "Path C ran and found nothing" from "Path C
did not run." Field names must not collide with Path A's structured log fields.

This requirement is specified in RISK-TEST-STRATEGY R-06 and ADR-003 but is absent from
SPECIFICATION.md. This brief mandates it.

### WARN-04: write_nli_edge Must NOT Be Modified

ARCHITECTURE.md says `write_nli_edge` is "refactored to delegate to `write_graph_edge`."
SPECIFICATION.md FR-12 says `write_nli_edge` is "NOT modified."

**The spec wins. `write_nli_edge` must NOT be modified.**

Add `write_graph_edge` as a NEW sibling function alongside `write_nli_edge`. Do not refactor
`write_nli_edge` to delegate. The risk of silently retagging existing NLI edges (R-02) by
an incorrect delegation is the reason for this constraint. A direct unit test must confirm
that `write_nli_edge(...)` still produces `source = 'nli'` after the change.

### R-13: Config Merge Function Must Be Updated

The config merge function in `config.rs` must include `supports_cosine_threshold` following
the `nli_informs_cosine_floor` pattern (f32 epsilon comparison for project-level override
propagation). Grep for `nli_informs_cosine_floor` in `config.rs` to locate the merge site.
Delivery must add `supports_cosine_threshold` at that exact site. Missing this causes
project-level overrides to be silently ignored.

---

## Constraints

| Constraint | Source |
|-----------|--------|
| No new HNSW scan in Path C — reuse `candidate_pairs` from Phase 4 | SCOPE.md, SPECIFICATION.md NFR-01 |
| `informs_category_pairs` reuse mandatory — no new allow-list config field | SCOPE.md, SPECIFICATION.md FR-02 |
| `write_nli_edge` must NOT be modified (see WARN-04 above) | SPECIFICATION.md FR-12 |
| No `score_batch`, `rayon_pool.spawn()`, or `spawn_blocking` in Path C | SPECIFICATION.md NFR-02 |
| No schema migration — `graph_edges.source` column (TEXT) already exists | SCOPE.md, SPECIFICATION.md |
| `run_graph_inference_tick` is infallible (returns `()`). No `?`, no `unwrap` in Path C | SPECIFICATION.md FR-15 |
| `inferred_edge_count` backward compat — continues to count `source='nli'` only | SPECIFICATION.md NFR-06 |
| `UNIQUE(source_id, target_id, relation_type)` does NOT include `source` — INSERT OR IGNORE is correct dedup | ARCHITECTURE.md, confirmed from DDL |
| `supports_candidate_threshold` (default 0.50) must remain <= `supports_cosine_threshold` (default 0.65) for Path C to receive candidates | IR-02 |
| Budget counter incremented only on `true` return from `write_graph_edge` | RISK-TEST-STRATEGY failure modes |
| `write_graph_edge` returning `false` (UNIQUE conflict) is NOT an error — must not emit warn! | R-07 |
| `!weight.is_finite()` guard required before threshold comparison in Path C | ARCHITECTURE.md error handling, R-09 |
| 500-line rule: if Path C adds enough volume that the tick function body exceeds ~150 lines, extract to `run_cosine_supports_path(...)` private helper in the same file | SPECIFICATION.md NFR-07 |
| Module rename (`nli_detection_tick.rs` → `graph_inference_tick.rs`) is deferred to Group 3 | SCOPE.md |
| crt-041 also touches `InferenceConfig` — rebase and verify struct literal after crt-041 merges if crt-041 is first | RISK-TEST-STRATEGY IR-03 |

---

## Dependencies

### Crates (no new crate dependencies)
- `unimatrix-store` — `graph_edges` table, `EDGE_SOURCE_*` constants, `GraphCohesionMetrics`, `Store::write_pool_server()`
- `unimatrix-server` — `InferenceConfig`, `run_graph_inference_tick`, `nli_detection.rs`, `nli_detection_tick.rs`
- `sqlx` — `INSERT OR IGNORE` execution, parameterized queries
- `serde` / `toml` — config deserialization (serde backing fn pattern)
- `tracing` — `warn!` and `debug!` logging in Path C and error paths

### Prerequisite Features
- **crt-039** (merged, PR #486) — `structural_graph_tick` runs unconditionally. Confirmed. This is the required condition for Path C.

### External Services
None — Path C is pure-cosine, no NLI model calls.

---

## NOT in Scope

- NLI Supports path (Path B) — Phase 6/7/8 and `supports_edge_threshold` are not modified
- PPR expander (Group 4) — graph enrichment enables it; implementation is not in this feature
- S1, S2, S8 edge sources — only cosine Supports (Path C) is in scope
- `signal_origin` schema column — the existing `source` column serves this role; no DDL change
- Same-feature-cycle guard — not required per ASS-035 Group D
- Same-category Supports pairs — only `informs_category_pairs` cross-category pairs
- `inferred_edge_count` rename — semantically stale after this feature; follow-up issue only
- Module rename (`nli_detection_tick.rs` → `graph_inference_tick.rs`) — deferred to Group 3
- `supports_category_pairs` config field — extension point for follow-on features (SR-05)
- Config-promotion of `MAX_COSINE_SUPPORTS_PER_TICK` — hardcoded constant; deferred
- Contradiction detection path — unchanged

---

## Alignment Status

**Overall: PASS with 4 variances requiring delivery attention.**

| Variance | Status | Resolution in Brief |
|---------|--------|-------------------|
| WARN-01: Category resolution HashMap not formally mandated in ACs | Addressed | See "Critical Implementation Mandates" — HashMap pre-build is MANDATORY; per-pair DB lookup prohibited |
| WARN-02: Path C observability log absent from spec | Addressed | See "Critical Implementation Mandates" — unconditional `debug!` log with `cosine_supports_candidates` and `cosine_supports_edges_written` is MANDATORY |
| WARN-03: AC-08 typo in SPECIFICATION.md (`EDGE_SOURCE_CO_ACCESS` should be `EDGE_SOURCE_COSINE_SUPPORTS`) | Corrected in brief | ACCEPTANCE-MAP.md uses the correct constant name; spec typo does not propagate |
| WARN-04: Architecture says "refactored to delegate"; spec says "NOT modified" | Resolved | Spec wins — `write_nli_edge` must NOT be modified; see "Critical Implementation Mandates" |

Vision alignment: PASS. crt-040 directly advances Wave 1 / W1-1 typed graph — `Supports` edge type is the entailment backbone required before the PPR expander (Group 4) and GNN training signal pipeline (W3-1) are meaningful.

---

## Test Coverage Summary (from RISK-TEST-STRATEGY)

| Priority | Risk | Key Test Requirement |
|----------|------|---------------------|
| Critical | R-01: Category data gap | Unit test: HashMap lookup succeeds for qualifying pair, `None` branch continues without panic, disallowed category pair above threshold produces no edge |
| High | R-02: write_nli_edge mis-tagging | Unit test: `write_nli_edge(...)` still writes `source='nli'`; `write_graph_edge(..., "cosine_supports")` writes `source='cosine_supports'` |
| High | R-03: impl Default / serde divergence | Three independent tests: `impl Default` path, serde path, and backing function — all assert 0.65 |
| Medium | R-06: Observability when empty | Log fires with zero counts when `candidate_pairs` is empty |
| Medium | R-07: Path B+C collision treated as error | `write_graph_edge` returning `false` (UNIQUE conflict) emits no warn and does not increment budget counter |
| Medium | R-09: NaN/Inf cosine guard | Pair with `cosine = f32::NAN` produces no edge, emits warn, loop continues |
| Medium | R-13: Config merge function | Unit test: project-level `supports_cosine_threshold = 0.70` overrides base `0.65` after merge |
| Medium | AC-12: Budget cap | 60 qualifying pairs → exactly 50 edges written |
| Low | R-09: NaN/Inf | Guard placed before threshold comparison |
| Eval gate | AC-14: MRR >= 0.2875 | `python product/research/ass-039/harness/run_eval.py` after delivery with Path C active |
