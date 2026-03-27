# crt-029 Implementation Brief — Background Graph Inference (Supports Edges)

GH Issue: #412

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-029/SCOPE.md |
| Architecture | product/features/crt-029/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-029/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-029/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-029/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| InferenceConfig additions + pub(crate) promotions + mod declaration | pseudocode/inference-config.md | test-plan/inference-config.md |
| Store::query_entries_without_edges + Store::query_existing_supports_pairs | pseudocode/store-query-helpers.md | test-plan/store-query-helpers.md |
| run_graph_inference_tick + select_source_candidates + write_inferred_edges_with_cap | pseudocode/nli-detection-tick.md | test-plan/nli-detection-tick.md |
| background.rs call site | pseudocode/background-call-site.md | test-plan/background-call-site.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/crt-029/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/crt-029/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Add a recurring background pass (`run_graph_inference_tick`) that systematically fills `Supports`
graph edges across the full active entry population using the existing NLI cross-encoder and HNSW
index. The tick is Supports-only — it MUST NOT write `Contradicts` edges; that is the dedicated
contradiction detection path's responsibility. This fixes the isolation of pre-NLI entries and
cross-category pairs that the post-store path (`run_post_store_nli`) never reaches, directly
improving PPR-based retrieval quality and reducing the `isolated_entry_count` visible in col-029
`context_status` metrics.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| New module vs. inline in `nli_detection.rs` | Mandatory new file `nli_detection_tick.rs` — `nli_detection.rs` is 1,373 lines, not ~650; adding inline would push it to ~1,600-1,700 lines; split is mandatory, not a judgment call | Architecture §Component 3 | architecture/ADR-001-nli-detection-tick-module-split.md |
| Reuse `write_edges_with_cap` vs. named variant | Named variant `write_inferred_edges_with_cap` — existing function takes one source + `Vec<(neighbor_id, text)>`; tick path has flat `Vec<(u64, u64)>` mixed pairs; threshold semantics differ (0.7 vs 0.6 default); wrapping would obscure cap boundary and make testing awkward | Architecture §Component 3 | architecture/ADR-002-write-inferred-edges-variant.md |
| Separate `max_source_candidates_per_tick` config field | Derived from `max_graph_inference_per_tick` — same cap applied to source selection as to NLI pairs; avoids a second knob for the same concern; misconfig risk (e.g. sources=1000, pairs=100 → 1000 O(N) embedding scans) eliminated | Architecture §Component 3 | architecture/ADR-003-source-candidate-bound-derived.md |
| Reuse `query_graph_edges()` vs. new store helper for pre-filter | New `Store::query_existing_supports_pairs()` — targeted SQL against `UNIQUE(source_id, target_id, relation_type)` index returns only `(source_id, target_id)` for non-bootstrap Supports rows; avoids loading all edge types per tick | Architecture §Component 2 | architecture/ADR-004-query-existing-supports-pairs.md |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/services/nli_detection_tick.rs` | Create | New module: `run_graph_inference_tick`, `select_source_candidates`, `write_inferred_edges_with_cap`, inline tests |
| `crates/unimatrix-server/src/services/mod.rs` | Modify | Add `pub mod nli_detection_tick;` declaration |
| `crates/unimatrix-server/src/services/nli_detection.rs` | Modify | Promote `write_nli_edge`, `format_nli_metadata`, `current_timestamp_secs` from `fn` to `pub(crate) fn` |
| `crates/unimatrix-server/src/services/background.rs` | Modify | Add `run_graph_inference_tick` call after `maybe_run_bootstrap_promotion`, gated on `nli_enabled` |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Add four new `InferenceConfig` fields, `Default` impl entries, `#[serde(default)]` attrs, and four `validate()` guards |
| `crates/unimatrix-store/src/read.rs` | Modify | Add `query_entries_without_edges()` and `query_existing_supports_pairs()` store helpers |

---

## Data Structures

### `InferenceConfig` — four new fields

```rust
pub supports_candidate_threshold: f32,   // HNSW pre-filter floor; default 0.5; range (0.0, 1.0)
pub supports_edge_threshold: f32,        // NLI entailment floor for Supports write; default 0.7; range (0.0, 1.0)
pub max_graph_inference_per_tick: usize, // pair cap per tick; default 100; range [1, 1000]
pub graph_inference_k: usize,            // HNSW neighbour count for tick path; default 10; range [1, 100]
```

All four require `#[serde(default = "...")]` attrs. All four must appear in `InferenceConfig`'s
struct-literal `Default` impl. `supports_candidate_threshold` is independent of
`nli_entailment_threshold` (post-store path). `graph_inference_k` is independent of
`nli_post_store_k` (tick path is background, not latency-sensitive).

### `ActiveEntryMeta` (ad-hoc, no named type required)

Fields used from `EntryRecord` in the tick: `id: u64`, `category: String`. The implementation
agent may pass `&[EntryRecord]` slices directly rather than defining a separate struct.

### `HashSet<(u64, u64)>` — existing Supports pairs pre-filter

Built from `query_existing_supports_pairs()`. Pairs are normalised as `(min(a,b), max(a,b))`
for deduplication. Used in Phase 4 to skip pairs already having a Supports edge.

---

## Function Signatures

### New public interface (`nli_detection_tick.rs`)

```rust
pub async fn run_graph_inference_tick(
    store: &Store,
    nli_handle: &NliServiceHandle,
    vector_index: &VectorIndex,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
)
```

### Private helpers (`nli_detection_tick.rs`)

```rust
// Selects up to max_sources source IDs in priority order (cross-category first,
// isolated second, remainder by created_at desc). Bounds get_embedding calls.
fn select_source_candidates(
    all_active: &[EntryRecord],          // or ActiveEntryMeta slice
    existing_edge_set: &HashSet<(u64, u64)>,
    isolated_ids: &HashSet<u64>,
    max_sources: usize,
) -> Vec<u64>

// Writes Supports edges ONLY from scored pairs; returns edges_written.
// Supports-only: no contradiction_threshold parameter, no Contradicts writes (C-13 / AC-10a).
// The dedicated contradiction detection path is the sole Contradicts writer.
async fn write_inferred_edges_with_cap(
    store: &Store,
    pairs: &[(u64, u64)],
    nli_scores: &[NliScores],
    supports_threshold: f32,        // config.supports_edge_threshold
    max_edges: usize,
) -> usize
```

### New store helpers (`unimatrix-store/src/read.rs`)

```rust
pub async fn query_entries_without_edges(&self) -> Result<Vec<u64>>
pub async fn query_existing_supports_pairs(&self) -> Result<HashSet<(u64, u64)>>
```

### Promoted to `pub(crate)` in `nli_detection.rs`

```rust
pub(crate) async fn write_nli_edge(store, source_id, target_id, relation_type, weight, created_at, metadata) -> bool
pub(crate) fn format_nli_metadata(scores: &NliScores) -> String
pub(crate) fn current_timestamp_secs() -> u64
```

### `background.rs` call site addition

```rust
if inference_config.nli_enabled {
    run_graph_inference_tick(store, nli_handle, vector_index, ml_inference_pool, inference_config).await;
}
```

---

## Algorithmic Design: `run_graph_inference_tick` (8 phases)

**Phase 1 — Guard (O(1))**: `nli_handle.get_provider().await`; return immediately on `Err`.

**Phase 2 — Data fetch (async DB)**: Three sequential reads:
1. `query_by_status(Active)` → `Vec<EntryRecord>` (id + category fields needed)
2. `query_entries_without_edges()` → `HashSet<u64>` (isolated IDs)
3. `query_existing_supports_pairs()` → `HashSet<(u64, u64)>` (pre-filter skip set)

**Phase 3 — Source candidate selection (cap BEFORE embedding)**: `select_source_candidates` →
`Vec<u64>` bounded to `max_graph_inference_per_tick`. Priority: (1) cross-category entries,
(2) isolated entries, (3) remaining by `created_at` desc. No `get_embedding` calls in this
phase — operates on metadata only (IDs, category strings). The cap is applied HERE so that
Phase 4 calls `get_embedding` only for the already-capped list. This is the critical ordering:
cap first (Phase 3), then fetch embeddings (Phase 4). (AC-06c / R-02)

**Phase 4 — HNSW expansion (embeddings for capped list only)**: For each source candidate
already capped in Phase 3 (in order): call `get_embedding(id).await`
(skip if `None`); call `search(emb, graph_inference_k, EF_SEARCH=32).await`; collect
`(source_id, neighbour_id, similarity)` triples where `similarity > supports_candidate_threshold`;
deduplicate `(A,B)` == `(B,A)` by normalising to `(min, max)`; skip pairs in pre-filter set.

**Phase 5 — Priority sort and truncation**: Sort collected pairs: cross-category first, either
endpoint isolated second, similarity desc third. Truncate to `max_graph_inference_per_tick`.

**Phase 6 — Text fetch**: For each pair `(source_id, target_id)`: fetch `content` via
`store.get_content_via_write_pool()` (matches bootstrap promotion pattern to see recent rows).
Skip pairs where either content cannot be fetched; log at `debug`.

**Phase 7 — W1-2 dispatch (single rayon spawn) — R-09 CRITICAL**: Collect all
`(source_text, target_text)` pairs; dispatch as ONE `rayon_pool.spawn()` closure calling
`provider.score_batch(&pairs)`. Await result. A second spawn for remaining pairs is explicitly
prohibited (Unimatrix entry #3653).

CRITICAL — R-09 rayon/tokio boundary (C-14): the closure body MUST be a synchronous CPU-bound
closure only. PROHIBITED inside the closure: `tokio::runtime::Handle::current()`, `.await`
expressions, any function that internally awaits or accesses the Tokio runtime. Rayon worker
threads have no Tokio runtime; any violation panics at runtime. This is compile-invisible.
Pre-merge gate: `grep -n 'Handle::current' crates/unimatrix-server/src/services/nli_detection_tick.rs`
must return empty inside any rayon closure. Independent validator required — not the author.

**Phase 8 — Write (Supports only)**: Call `write_inferred_edges_with_cap(store, &scored_pairs,
&nli_scores, supports_edge_threshold, max_graph_inference_per_tick)`. No `contradiction_threshold`
parameter — the tick writes Supports edges only (C-13 / AC-10a). Log `edges_written` at `debug`.

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | W1-2 (hard): all `CrossEncoderProvider::score_batch` calls MUST go via `rayon_pool.spawn()`. `spawn_blocking` prohibited. Inline async NLI prohibited. Violation blocks tokio executor and is a gate-3c failure. |
| C-02 | SQLite via sqlx only. `read_pool()` for reads; `write_pool_server()` for writes. No raw rusqlite. `query_entries_without_edges()` and `query_existing_supports_pairs()` use `read_pool()`. |
| C-03 | `INSERT OR IGNORE` idempotency. `UNIQUE(source_id, target_id, relation_type)` is the dedup backstop; pre-filter is an optimisation only. |
| C-04 | No schema migration. No new columns. No `ALTER TABLE`. No schema version bump. |
| C-05 | No new crate dependencies. Confined to existing workspace crates: `unimatrix-core`, `unimatrix-embed`, `unimatrix-store`, `sqlx`, `tokio`, `tracing`. |
| C-06 | `supports_edge_threshold` default 0.7 is intentionally higher than `nli_entailment_threshold` default 0.6. Both are independent fields. |
| C-07 | SUPERSEDED by C-13. The tick writes NO Contradicts edges. |
| C-13 | Tick MUST NOT write Contradicts edges. `write_inferred_edges_with_cap` is Supports-only and has no `contradiction_threshold` parameter. The dedicated contradiction detection path is the sole Contradicts writer. Any Contradicts write in `nli_detection_tick.rs` is a gate-3c failure (AC-10a). |
| C-14 | R-09 rayon/tokio boundary: the rayon closure in Phase 7 MUST be a synchronous CPU-bound closure only. `tokio::runtime::Handle::current()`, `.await`, and all async calls are PROHIBITED inside `rayon_pool.spawn()`. Rayon worker threads have no Tokio runtime; violations panic at runtime. Compile-invisible — requires grep detection and independent code review. |
| C-08 | File size hard limit: `nli_detection_tick.rs` must not exceed 800 lines. This is a merge gate condition (NFR-05). |
| C-09 | Supports-only inference. No `Prerequisite` write path in this feature. `RelationType::Prerequisite` exists in the type system but no tick write path is added here. |
| C-10 | `run_post_store_nli` is unchanged. The tick is additive only. |
| C-11 | Pre-PR grep: `grep -rn 'InferenceConfig {' crates/unimatrix-server/src/` — 52 occurrences must each include the four new fields or `..InferenceConfig::default()` tail. Known gate-failure pattern from crt-023 (entry #2730). |
| C-12 | `compute_graph_cohesion_metrics` uses `read_pool()` per entry #3619. Confirmed in architecture. See open item below on ADR conflict housekeeping. |

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `unimatrix-store` | Workspace crate | `query_by_status()`, `query_graph_edges()`, `EDGE_SOURCE_NLI`, new `query_entries_without_edges()`, new `query_existing_supports_pairs()` |
| `unimatrix-core` | Workspace crate | `VectorIndex::search()`, `VectorIndex::get_embedding()` |
| `unimatrix-embed` | Workspace crate | `CrossEncoderProvider::score_batch`, `NliScores` |
| `NliServiceHandle` | Internal (infra) | `get_provider()` — readiness gate |
| `RayonPool` | Internal (infra) | `spawn()` for W1-2 contract |
| `InferenceConfig` | Internal (infra/config.rs) | Four new fields added here |
| `maybe_run_bootstrap_promotion` | Internal (services/nli_detection.rs) | Tick runs after this; ordering required |
| `write_nli_edge` | Internal (nli_detection.rs, promoted) | Low-level INSERT; promoted to `pub(crate)` |
| `format_nli_metadata` | Internal (nli_detection.rs, promoted) | JSON serialisation of NliScores; promoted to `pub(crate)` |
| `current_timestamp_secs` | Internal (nli_detection.rs, promoted) | Unix epoch helper; promoted to `pub(crate)` |
| `EDGE_SOURCE_NLI` | `unimatrix_store::read:1563` | Already pub; canonical `"nli"` constant |
| col-029 graph cohesion metrics | `context_status` | Observability layer for tick output: `isolated_entry_count`, `cross_category_edge_count`, `inferred_edge_count` |
| col-030 `suppress_contradicts` | `SearchService::search` | Always-on; motivates the C-07 contradiction threshold floor |

---

## NOT in Scope

- `Prerequisite` edge inference (deferred to W3-1)
- `Prerequisite` bootstrap_only promotion
- Changes to `run_post_store_nli`
- Changes to `TypedRelationGraph` or `build_typed_relation_graph`
- Changes to search ranking or confidence scoring
- Changes to `contradiction::scan_contradictions` (status-diagnostic path, unaffected)
- Auto-quarantine triggers from the new tick
- New ONNX models or rayon pools
- Schema migration
- New crate dependencies
- Tick-modulo interval gate (the cap is the sole throttle)
- Alerting when `isolated_entry_count` changes

---

## Pre-Merge Gates (delivery agent checklist)

These are mandatory shell checks before the PR is opened:

```bash
# Gate 1 — struct literal coverage (AC-18†)
grep -rn 'InferenceConfig {' crates/unimatrix-server/src/
# Expected: 52 occurrences, each with new fields or ..default() tail

# Gate 2 — file size (NFR-05 / C-08)
wc -l crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: <= 800

# Gate 3 — no spawn_blocking in tick module (C-01 / AC-08)
grep -n 'spawn_blocking' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty

# Gate 4 — pub(crate) promotions present (R-11)
grep -n 'pub(crate) fn write_nli_edge\|pub(crate) fn format_nli_metadata\|pub(crate) fn current_timestamp_secs' \
  crates/unimatrix-server/src/services/nli_detection.rs
# Expected: all three present

# Gate 5 — pool choice in compute_graph_cohesion_metrics (C-12)
grep -n 'compute_graph_cohesion_metrics\|read_pool\|write_pool' \
  crates/unimatrix-store/src/read.rs
# Expected: function uses read_pool()

# Gate 6 — tick writes NO Contradicts edges (C-13 / AC-10a)
grep -n 'Contradicts' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty (no Contradicts writes; tick is Supports-only)

# Gate 7 — R-09 rayon/tokio boundary (C-14)
# Run inside nli_detection_tick.rs; any match inside a rayon closure is gate-blocking
grep -n 'Handle::current' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty
# Also: manual inspection of the rayon_pool.spawn() closure body for .await expressions
# REQUIRED: independent validator (not the author) must confirm the closure body is sync-only
```

---

## Alignment Status

**Overall: PASS with two non-blocking WARNs. No VARIANCE or FAIL.**

| WARN | Details | Delivery Impact |
|------|---------|----------------|
| WARN-1: `query_existing_supports_pairs()` not named in SCOPE.md | Architecture (ADR-004) introduces this as a necessary implementation detail for the pre-filter HashSet. SCOPE.md names only `query_entries_without_edges()` as a new store helper; the second helper is a reasonable decomposition, not a functional addition. The alternative (filter `query_graph_edges()` in Rust) is also acceptable per ARCHITECTURE.md §Open Questions item 2. | None. Both approaches are valid; delivery agent may choose. |
| WARN-2: R-06 ADR conflict (#3593 write-pool vs #3595 read-pool for `compute_graph_cohesion_metrics`) requires housekeeping | Architecture asserts `read_pool()` is correct (entry #3619). The conflicting ADR #3593 must be deprecated in Unimatrix before or during delivery. This is a knowledge integrity task, not a feature scope change. `compute_graph_cohesion_metrics` is confirmed to use `read_pool()` — no code change required. | Delivery agent: deprecate Unimatrix entry #3593 in wave-1 before any write-path work begins. |

---

## Open Questions for Human Review

None blocking delivery. The R-06 ADR conflict (WARN-2) is a knowledge housekeeping task that the delivery agent can resolve by deprecating Unimatrix entry #3593 at the start of wave-1. No human approval needed before coding begins.
