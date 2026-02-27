# crt-005: Coherence Gate

## Problem Statement

Unimatrix has four independent structural health signals scattered across its codebase, but no unified metric monitors them or acts on them. Each signal represents a form of knowledge base degradation that worsens over time if unaddressed:

1. **Confidence staleness.** `EntryRecord.confidence` is computed at mutation time (store/correct/deprecate/quarantine) and stored as f32. The freshness component uses `(-age_hours / 168.0).exp()`, but this value is never recomputed between mutations. An entry stored with freshness=1.0 retains that stale value indefinitely. The search re-ranking formula (`0.85 * similarity + 0.15 * confidence`) uses the stale value, systematically over-ranking old entries. The drift is small per entry (+0.027 max from phantom freshness) but systematic across the knowledge base.

2. **HNSW graph degradation.** When entries are re-embedded (via `context_correct` or embedding consistency checks), a new point is added to the HNSW graph but the old point remains as a stale routing node. `VectorIndex::stale_count()` tracks these (implemented in nxs-002) but nothing triggers cleanup. Over many correction cycles, stale nodes accumulate and degrade search quality -- the graph structure drifts from its optimal configuration.

3. **Embedding inconsistency.** The `check_embedding_consistency` function (crt-003) re-embeds entries and checks self-similarity, but the results are only surfaced as a list in `context_status` when explicitly opted in via `check_embeddings: true`. The ratio of inconsistent entries to total entries is not tracked as a coherence signal, and rising inconsistency rates (indicating model drift or content corruption) are not flagged.

4. **Contradiction density.** `context_status` performs contradiction scanning (crt-003) and reports individual contradiction pairs, but the ratio of quarantined entries to active entries -- a measure of knowledge base quality degradation -- is not tracked or trended.

5. **Scoring precision ceiling.** The confidence system computes internally in f64 for numerical stability (ADR-002) but truncates to f32 at the return boundary. All scoring constants (confidence weights, re-ranking weights, boost caps, thresholds) are f32, limiting effective precision to ~7 decimal digits. This makes pi-based calibration of scoring weights negligible (ass-012 finding) and produces JSON precision artifacts (e.g., `0.8500000238418579`). The f32 ceiling also constrains future scale: as the knowledge base grows, fine-grained score differentiation between entries with similar relevance requires more precision than f32 provides.

These five signals are independent health indicators that, today, require human inspection of raw `context_status` output to interpret. There is no composite metric, no threshold-based alerting, and no automated maintenance actions. The downstream consequence: col-002 (Retrospective Pipeline) draws conclusions from knowledge quality signals. Stale confidence and degraded HNSW graphs produce misleading retrospective insights because the data they analyze is not structurally sound.

## Goals

1. **Compute a composite coherence metric (lambda).** Define a single float value in [0.0, 1.0] combining all four dimension scores, where 1.0 = fully coherent and 0.0 = fully degraded. Expose as a `coherence` field in `StatusReport`.

2. **Expose individual dimension scores.** Each of the four dimensions produces a score in [0.0, 1.0], exposed in `StatusReport` for diagnostics. Dimensions: confidence freshness, graph quality, embedding consistency, contradiction density.

3. **Lazy confidence refresh.** During `context_status` (and optionally `context_search`), identify entries whose stored confidence age exceeds a staleness threshold (default 24h) and recompute their confidence inline. No background threads, no timers -- maintenance is piggybacked on existing tool calls.

4. **HNSW graph compaction trigger.** When the stale node ratio (from `VectorIndex::stale_count()` / `VectorIndex::point_count()`) exceeds a threshold (default 10%), trigger graph compaction during `context_status`. Compaction rebuilds the HNSW index from current active entries only, eliminating stale routing nodes.

5. **Embedding consistency as a continuous signal.** Extend the existing `check_embedding_consistency` infrastructure to track the inconsistency ratio as a dimension of lambda. When `context_status` runs with embedding checks enabled, the ratio feeds into the coherence score.

6. **Contradiction density as a continuous signal.** Compute the ratio of quarantined entries to total active entries from existing counter data. No new scanning required -- this uses the `total_quarantined` and `total_active` counters already maintained.

7. **Maintenance recommendations.** When lambda drops below a configurable threshold (default 0.8), the `context_status` response includes actionable maintenance recommendations (e.g., "N entries have stale confidence", "HNSW graph has X% stale nodes -- compaction recommended").

8. **Inline maintenance execution.** Confidence refresh and co-access staleness cleanup already execute inline during `context_status`. crt-005 adds HNSW compaction to this pattern. All maintenance operations execute during `context_status` calls -- no background threads, no new async patterns.

9. **Upgrade scoring pipeline from f32 to f64.** Promote `EntryRecord.confidence` from f32 to f64. Upgrade all scoring constants (confidence weights, re-ranking weights, boost caps, coherence thresholds) from f32 to f64. Remove the `as f32` truncation at the confidence computation return boundary. Upgrade `SearchResult.similarity` and all `StatusReport` score fields to f64. This requires a schema migration (v2 -> v3) for the `confidence` field in `EntryRecord`. Embeddings remain f32 — the ONNX model is the precision bottleneck there, and future scale means quantizing down (int8/binary), not up. The `update_confidence` Store method signature changes from f32 to f64.

## Non-Goals

- **No new MCP tools.** Lambda is exposed via the existing `context_status` response. No `context_coherence` or `context_maintenance` tool.
- **No background maintenance scheduler.** All maintenance is inline during `context_status` calls. The server has no background thread pool and crt-005 does not add one.
- **No automatic quarantine from coherence signals.** Low lambda triggers recommendations, not automatic actions. Quarantine remains a human/Admin decision (crt-003 design principle).
- **No new fields on EntryRecord.** Coherence is computed at query time from existing data. The only EntryRecord change is the `confidence` field type promotion from f32 to f64 (schema v2 -> v3).
- **No new redb tables.** All data needed for lambda computation already exists: COUNTERS for status counts, ENTRIES for confidence ages, VectorIndex for stale counts, existing contradiction/embedding infrastructure.
- **No confidence refresh during context_search.** The initial implementation limits lazy refresh to `context_status` only. Adding refresh during search would add write latency to the hot read path. This can be revisited based on usage data.
- **No persistence of lambda history.** The coherence score is computed on demand. Historical trending is mtx-phase work (dashboard charts).
- **No pi-based calibration of scoring weights in this scope.** The f64 upgrade makes pi-derived thresholds viable (ass-012), but weight selection is a separate concern. crt-005 upgrades the precision; future work may adopt pi-derived constants.
- **No embedding precision changes.** Embeddings remain f32 (384-dim vectors from ONNX). Future scale means quantizing down (int8/binary), not up. The f64 upgrade applies only to the scoring pipeline.
- **No adaptive precision lanes.** Multi-precision embedding storage is a future architecture concern, not crt-005 scope.
- **No graph compaction during context_search.** Compaction is expensive and only runs during `context_status`. Search calls check the stale ratio for the coherence signal but do not trigger compaction.

## Background Research

### Existing Infrastructure Inventory

**Confidence staleness (crt-002):**
- `compute_confidence(entry, now)` in `confidence.rs` -- six-factor additive composite, computes internally in f64 but returns f32 (truncation)
- `freshness_score(last_accessed_at, created_at, now)` -- exponential decay with 168h half-life
- Confidence is written on mutation (store, correct, deprecate, quarantine) and retrieval (fire-and-forget after usage recording)
- `update_confidence(id, confidence)` on Store -- targeted write without full index diff
- The freshness component becomes stale between accesses: stored value reflects time-of-last-mutation, not current time

**HNSW graph health (nxs-002):**
- `VectorIndex::stale_count()` -- returns `point_count() - id_map.active_count()`
- `VectorIndex::point_count()` -- total HNSW nodes including stale
- Stale nodes arise from `VectorIndex::insert()` during re-embed: new data_id allocated, old data_id removed from id_map but not from HNSW graph
- No compaction method exists on VectorIndex today -- this must be added
- `VectorIndex::dump()` and `load()` in persistence.rs handle serialization
- hnsw_rs does not support point removal; compaction requires full rebuild

**Embedding consistency (crt-003):**
- `check_embedding_consistency()` in `contradiction.rs` -- re-embeds all active entries, checks self-similarity >= 0.99
- Returns `Vec<EmbeddingInconsistency>` with entry_id, title, expected_similarity
- Opt-in via `check_embeddings: true` parameter on `context_status`
- `embedding_check_performed: bool` and `embedding_inconsistencies: Vec<EmbeddingInconsistency>` on StatusReport

**Contradiction/quarantine tracking (crt-003):**
- `total_quarantined: u64` counter in COUNTERS table
- `total_active: u64` counter in COUNTERS table
- `contradiction_count: usize` and `contradictions: Vec<ContradictionPair>` on StatusReport
- Contradiction scanning runs by default during `context_status` when embed service is ready

**Co-access maintenance (crt-004):**
- `cleanup_stale_co_access(staleness_cutoff)` runs during `context_status` -- established precedent for inline maintenance
- `stale_pairs_cleaned: u64` reported in StatusReport

### VectorIndex Compaction Design

hnsw_rs does not support individual point deletion. Compaction requires:
1. Read all current entry-to-data mappings from the id_map
2. For each active mapping, retrieve the stored embedding (via VECTOR_MAP -> entry_id -> re-embed from content)
3. Create a fresh HNSW index with the same configuration
4. Insert all active embeddings into the new index
5. Replace the old index with the new one
6. Update VECTOR_MAP with new data IDs

This is an O(n log n) operation where n = active entries. At Unimatrix's current scale (<1000 entries), this takes <1 second. At 10K entries, ~5-10 seconds. The operation should be gated by the stale ratio threshold to avoid running on every `context_status` call.

Alternative: re-embed from stored content rather than reading raw embeddings from HNSW. This is more robust (embeddings are regenerated from current model) but more expensive (requires embed service). Decision deferred to architecture phase.

### Lambda Composition

The product vision specifies a composite lambda in [0.0, 1.0] combining four dimensions. Each dimension produces a score in [0.0, 1.0] where 1.0 = healthy:

| Dimension | Score = 1.0 | Score = 0.0 | Data Source |
|-----------|-------------|-------------|-------------|
| Confidence freshness | All entries have confidence computed within staleness threshold | All entries are stale | ENTRIES scan: compare `updated_at` to now |
| Graph quality | No stale HNSW nodes | All nodes are stale | `stale_count()` / `point_count()` |
| Embedding consistency | All entries pass self-similarity check | All entries inconsistent | `check_embedding_consistency` results |
| Contradiction density | No quarantined entries | All entries quarantined | `total_quarantined` / `total_active` counters |

Composite lambda: weighted average of dimension scores. Weights TBD in architecture phase. Simple equal weighting (0.25 each) is a reasonable default. The contradiction density and embedding consistency dimensions may warrant lower weights since they require expensive scans that may not run on every `context_status` call.

### Staleness Detection for Confidence

To determine which entries have stale confidence, we need to know when confidence was last computed. Currently, confidence is recomputed on every retrieval (fire-and-forget after usage recording) and on mutations. The `updated_at` field reflects the last mutation time. The `last_accessed_at` field reflects the last retrieval time. The more recent of these two timestamps represents when confidence was last recomputed.

Entries where `max(updated_at, last_accessed_at)` is older than the staleness threshold (default 24h) have stale confidence.

### ass-012 Research Integration

The ass-012 research spike investigated pi-based calibration and the lambda coherence gate concept. Key findings relevant to crt-005:

1. **Lambda coherence gate is the highest-value concept** -- a unified metric gating self-maintenance maps directly to Unimatrix's architecture
2. **Stale confidence drift is a real current issue** -- entries retain phantom freshness values, causing systematic over-ranking
3. **HNSW graph degradation is a real current issue** -- stale nodes accumulate from re-embeds, degrading search quality
4. **Pi-calibration of scoring weights was negligible at f32** -- but with the f64 upgrade, pi-derived thresholds become viable for future work (15 digits of precision vs 7)

### f32 -> f64 Scoring Upgrade

The scoring pipeline currently truncates f64 computation results to f32 at the return boundary. This is a systemic precision ceiling:

- `compute_confidence()` computes in f64, returns f32 (ADR-002 established f64 internal computation)
- `co_access_affinity()` computes in f64, returns f32
- All weight constants (`W_BASE`, `W_USAGE`, etc.) are f32
- `SearchResult.similarity` is f32
- `EntryRecord.confidence` is stored as f32 (4 bytes via bincode)
- `rerank_score()` takes f32 inputs and returns f32

The upgrade promotes the entire scoring pipeline to f64:
- `EntryRecord.confidence: f64` -- requires schema migration v2 -> v3 (4 -> 8 bytes per entry)
- `SearchResult.similarity: f64` -- in-memory only, no persistence impact
- All weight/threshold constants become f64 -- compile-time change
- `update_confidence(id, f64)` -- Store trait signature change
- JSON responses emit full f64 precision -- eliminates `0.8500000238418579` artifacts

**Embeddings remain f32.** ONNX models output f32. The hnsw_rs index uses `Hnsw<'static, f32, DistDot>` with SIMD-optimized f32 operations. Upcasting to f64 would double memory with no precision gain (the model is the bottleneck). Future embedding scale means quantizing DOWN (int8/binary), not up.

## Proposed Approach

### StatusReport Extension

Add new fields to `StatusReport` (all scores f64):
- `coherence: f64` -- composite lambda score [0.0, 1.0]
- `confidence_freshness_score: f64` -- dimension 1 [0.0, 1.0]
- `graph_quality_score: f64` -- dimension 2 [0.0, 1.0]
- `embedding_consistency_score: f64` -- dimension 3 [0.0, 1.0]
- `contradiction_density_score: f64` -- dimension 4 [0.0, 1.0]
- `stale_confidence_count: u64` -- entries with stale confidence
- `confidence_refreshed_count: u64` -- entries refreshed during this call
- `graph_stale_ratio: f64` -- current stale node ratio
- `graph_compacted: bool` -- whether compaction ran during this call
- `maintenance_recommendations: Vec<String>` -- actionable recommendations when lambda < threshold

### Coherence Module

New module `coherence.rs` in `unimatrix-server` containing:
1. Dimension score computation functions (four pure functions)
2. Composite lambda computation (weighted average)
3. Maintenance recommendation generation
4. Threshold constants (staleness window, stale ratio trigger, lambda threshold)

### Confidence Refresh

During `context_status`, after the read transaction:
1. Scan all active entries (already done for correction chain metrics)
2. Identify entries where `max(updated_at, last_accessed_at)` < `now - staleness_threshold`
3. Recompute confidence for stale entries using `compute_confidence(entry, now)`
4. Batch-write updated confidence values via `update_confidence`
5. Report count of refreshed entries

### Graph Compaction

New method on `VectorIndex`:
1. Check stale ratio: `stale_count() / point_count()`
2. If ratio > threshold, rebuild index from active entries
3. Requires embed service to re-embed active entries (or read from stored embeddings if accessible)
4. Replace internal HNSW index and id_map atomically
5. Update VECTOR_MAP in store

### Integration into context_status

The existing `context_status` handler flow becomes:
1. Read transaction for counters, distributions, correction metrics (existing)
2. Contradiction scanning (existing)
3. Embedding consistency check if opted in (existing)
4. **NEW: Compute dimension scores from available data**
5. **NEW: Confidence refresh for stale entries**
6. **NEW: HNSW compaction if stale ratio exceeds threshold**
7. Co-access stats and cleanup (existing)
8. **NEW: Compute composite lambda and generate recommendations**
9. Build final StatusReport with coherence fields

## Acceptance Criteria

- AC-01: `StatusReport` includes a `coherence: f64` field in [0.0, 1.0] representing the composite lambda score
- AC-02: `StatusReport` includes four individual dimension scores (`confidence_freshness_score`, `graph_quality_score`, `embedding_consistency_score`, `contradiction_density_score`), each f64 in [0.0, 1.0]
- AC-03: Confidence freshness dimension is computed as the ratio of entries with non-stale confidence to total active entries, where staleness is defined as `max(updated_at, last_accessed_at)` older than a configurable threshold (default 24h)
- AC-04: Graph quality dimension is computed as `1.0 - (stale_count / point_count)`, clamped to [0.0, 1.0]
- AC-05: Embedding consistency dimension is computed as `1.0 - (inconsistent_count / total_checked)` when embedding checks are performed; defaults to 1.0 (healthy) when checks are not performed
- AC-06: Contradiction density dimension is computed as `1.0 - (total_quarantined / total_active)`, clamped to [0.0, 1.0]; returns 1.0 when total_active is 0
- AC-07: Composite lambda is a weighted average of the four dimension scores with configurable weights summing to 1.0
- AC-08: When lambda < configurable threshold (default 0.8), `StatusReport` includes non-empty `maintenance_recommendations` with actionable text
- AC-09: Entries with stale confidence (exceeding the staleness threshold) have their confidence recomputed during `context_status` execution
- AC-10: `confidence_refreshed_count` in StatusReport reflects the number of entries whose confidence was refreshed during this call
- AC-11: HNSW graph compaction is triggered during `context_status` when the stale node ratio exceeds a configurable threshold (default 10%)
- AC-12: Graph compaction rebuilds the HNSW index from active entries only, eliminating all stale routing nodes
- AC-13: After compaction, `VectorIndex::stale_count()` returns 0 and VECTOR_MAP entries are updated with new data IDs
- AC-14: `graph_compacted: bool` in StatusReport indicates whether compaction ran during this call
- AC-15: All dimension score computation functions are pure (deterministic, no side effects) and independently unit-testable
- AC-16: All threshold constants (staleness window, stale ratio trigger, lambda threshold, dimension weights) are named constants, not magic numbers
- AC-17: `context_status` response formatting includes coherence section in all three formats (summary, markdown, json)
- AC-18: Maintenance recommendations are specific and actionable (e.g., "42 entries have stale confidence (oldest: 7 days)", not generic platitudes)
- AC-19: Confidence refresh does not block the `context_status` response beyond reasonable limits; large refresh batches are capped per call
- AC-20: Graph compaction does not corrupt the HNSW index -- search results before and after compaction return the same entries for the same query (modulo stale node removal)
- AC-21: All new code has unit tests; integration tests verify end-to-end coherence computation, confidence refresh, and graph compaction
- AC-22: Existing tests continue to pass with no regressions
- AC-23: `#![forbid(unsafe_code)]`, no new crate dependencies beyond existing workspace
- AC-24: No background threads, no timers, no new async patterns -- all operations inline during `context_status`
- AC-25: `EntryRecord.confidence` is f64. Schema migration v2 -> v3 converts existing f32 confidence values to f64 losslessly
- AC-26: `SearchResult.similarity` is f64. Vector search returns f64 similarity scores
- AC-27: All scoring constants (confidence weights, re-ranking weights, boost caps, coherence thresholds) are f64
- AC-28: `compute_confidence()` returns f64 (no `as f32` truncation at the return boundary)
- AC-29: `update_confidence()` Store method accepts f64
- AC-30: `rerank_score()` operates entirely in f64
- AC-31: Embeddings remain f32 throughout (ONNX pipeline, VectorIndex, HNSW). Only the scoring pipeline is upgraded
- AC-32: Existing entries are readable after migration; schema v2 data deserializes correctly into the v3 schema

## Constraints

- **No new MCP tools.** Lambda is exposed via existing `context_status` response.
- **No background threads.** The server has no scheduler. All maintenance inline during `context_status`.
- **No new crate dependencies.** All functionality uses existing workspace crates.
- **`#![forbid(unsafe_code)]`**, edition 2024, MSRV 1.89.
- **Object-safe traits.** Any trait extensions must maintain object safety.
- **hnsw_rs does not support point deletion.** Compaction requires full rebuild of the HNSW index.
- **Schema migration v2 -> v3.** `EntryRecord.confidence` promoted from f32 to f64. Migration reads existing f32 values and writes f64 losslessly (`f32 as f64` is exact). No new fields added.
- **No new redb tables.** All data sources already exist.
- **Confidence refresh is a write operation during a diagnostic call.** `context_status` is a read-heavy diagnostic tool; adding writes must be bounded (cap refresh batch size).
- **Test infrastructure is cumulative.** Build on existing fixtures and helpers.
- **VectorStore trait is object-safe.** If compaction requires a new trait method, it must be object-safe.
- **Graph compaction requires embed service.** If the embed service is not available (lazy loading), compaction cannot run. Graceful degradation.

## Open Questions

1. **Should confidence refresh also run during `context_search`?** This would keep confidence fresh on the hot path but adds write latency. Proposed as non-goal initially. If `context_status` is called regularly (e.g., by monitoring), it may be sufficient.

2. **Should graph compaction re-embed from content or read raw embeddings?** Re-embedding from content is more robust (uses current model) but requires the embed service and is slower. Reading raw embeddings from the HNSW index would be faster but the hnsw_rs API may not expose raw vectors for specific data points. Deferred to architecture.

3. **What are appropriate dimension weights for composite lambda?** Equal weighting (0.25 each) is simple but may not reflect relative importance. Confidence freshness and graph quality are always available; embedding consistency requires opt-in. Should unavailable dimensions be excluded from the weighted average or default to 1.0 (healthy)?

## Dependencies

- **Depends on:** col-001 (Outcome Tracking) -- complete; crt-001 through crt-004 -- all complete
- **Blocks:** col-002 (Retrospective Pipeline) -- needs reliable knowledge quality signals

## Tracking

https://github.com/dug-21/unimatrix/issues/44
