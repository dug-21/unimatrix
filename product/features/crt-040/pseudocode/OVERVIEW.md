# crt-040 Pseudocode Overview — Cosine Supports Edge Detection

## Feature Summary

Restore `Supports` edge production by implementing Path C (pure-cosine detection) inside
`run_graph_inference_tick`. No new HNSW scan. No NLI model. Path C reuses the Phase 4
`candidate_pairs` already computed for Path B, applies a cosine threshold + category filter,
and writes `Supports` edges tagged `source = 'cosine_supports'`.

---

## Components Involved

| Component | File | Wave | Pseudocode File |
|-----------|------|------|-----------------|
| Store constant | `crates/unimatrix-store/src/read.rs` + `lib.rs` | 1a | store-constant.md |
| InferenceConfig | `crates/unimatrix-server/src/infra/config.rs` | 1b | inference-config.md |
| Edge writer | `crates/unimatrix-server/src/services/nli_detection.rs` | 2 | write-graph-edge.md |
| Path C loop | `crates/unimatrix-server/src/services/nli_detection_tick.rs` | 3 | path-c-loop.md |

---

## Wave Dependency Order

```
Wave 1a  →  Wave 1b  →  Wave 2  →  Wave 3
```

- **Wave 1a** defines `EDGE_SOURCE_COSINE_SUPPORTS`. Wave 2 imports it; Wave 3 imports it.
  Must land first so the constant is visible during compilation of downstream waves.
- **Wave 1b** adds `supports_cosine_threshold` to `InferenceConfig`. Wave 3 reads
  `config.supports_cosine_threshold`. Must compile before Wave 3.
- **Wave 2** defines `write_graph_edge`. Wave 3 calls it. Must compile before Wave 3.
- **Wave 3** is the integrating change. Depends on all three prior waves being present.

All four waves may be implemented in a single commit, but the sequencing constraint above
must be respected if the delivery agent chooses to stage commits.

---

## Data Flow

```
Phase 2 (DB reads)
  store.query_by_status("active") → all_active: Vec<EntryRecord>
  store.query_existing_supports_pairs() → existing_supports_pairs: HashSet<(u64, u64)>

Phase 2 post-processing (NEW in Wave 3 — built once, before Path C)
  all_active → category_map: HashMap<u64, String>  // entry_id → category

Phase 4 (HNSW scan — UNCHANGED, reused by Path C)
  candidate_pairs: Vec<(u64, u64, f32)>
    canonical form: source_id < target_id (lo, hi)
    sorted by: cross-category first, then isolated endpoint, then similarity desc
    truncated to: config.max_graph_inference_per_tick

PATH A (Informs write loop — UNCHANGED)
  informs_metadata → write_nli_edge(source='nli', rel='Informs') × N

PATH C (NEW — Wave 3)
  for (src_id, tgt_id, cosine) in candidate_pairs:
    guard: !cosine.is_finite()  → warn, continue
    gate 1: cosine >= config.supports_cosine_threshold (0.65 default) → else continue
    gate 2: cosine_supports_written >= MAX_COSINE_SUPPORTS_PER_TICK (50) → break
    gate 3: category_map[src_id], category_map[tgt_id] must be in informs_category_pairs
    gate 4: (src_id.min(tgt_id), src_id.max(tgt_id)) not in existing_supports_pairs
    write: write_graph_edge(store, src_id, tgt_id, "Supports", cosine, ts,
                            EDGE_SOURCE_COSINE_SUPPORTS, '{"cosine": <f32>}')
    on true (rows_affected=1, new insert): cosine_supports_written += 1
    on false (rows_affected=0, UNIQUE conflict): continue, no warn, no counter increment
    on false (SQL error): continue, no additional warn (already logged inside fn), no counter increment
  unconditional debug! log after loop — fires even when candidate_pairs is empty (AC-19)

PATH B entry gate (UNCHANGED — guards NLI batch only)
  // NOTE: the Phase 5 joint early-return (candidate_pairs.is_empty() && informs_metadata.is_empty())
  // is REMOVED by this feature so Path C's observability log fires unconditionally (AC-19).
  // This Path B gate is RETAINED — it is positioned after Path C and guards Phase 6/7/8 only.
  if candidate_pairs.is_empty() { return; }
  get_provider() → Err = early return
  [Phase 6/7/8: NLI Supports — unchanged]
```

---

## Shared Types and New Symbols

### New constant (Wave 1a)

```
EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports"
  defined in: unimatrix-store/src/read.rs
  re-exported from: unimatrix-store/src/lib.rs (alongside EDGE_SOURCE_NLI, EDGE_SOURCE_CO_ACCESS)
```

### New function (Wave 2)

```
write_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f32,
    created_at: u64,
    source: &str,
    metadata: &str,
) -> bool
  defined in: unimatrix-server/src/services/nli_detection.rs
  visibility: pub(crate)
  write_nli_edge: NOT MODIFIED — remains independent, hardcodes source='nli'
```

### New config field (Wave 1b)

```
InferenceConfig.supports_cosine_threshold: f32
  default: 0.65 (via serde backing fn AND impl Default literal — dual-site, ADR-002)
  range: (0.0, 1.0) exclusive
  merge: follows nli_informs_cosine_floor f32 epsilon comparison pattern
```

### New module constant (Wave 3)

```
MAX_COSINE_SUPPORTS_PER_TICK: usize = 50
  defined in: nli_detection_tick.rs (adjacent to MAX_INFORMS_PER_TICK = 25)
```

### Local runtime state (Wave 3 — tick-scoped)

```
category_map: HashMap<u64, String>
  built once from all_active after Phase 2
  key: entry_id (u64)   value: category (String)
  per-pair DB lookup is PROHIBITED — HashMap pre-build is MANDATORY (WARN-01)

cosine_supports_written: usize   (budget counter — incremented only on true return)
cosine_supports_candidates: usize  (threshold-passing pairs, for observability log)
```

### graph_edges row written by Path C (no schema change)

```
source_id:      u64   (lo of canonical pair)
target_id:      u64   (hi of canonical pair)
relation_type:  "Supports"
weight:         f32   (= cosine value, no multiplier — ADR resolved decision 3)
source:         EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports"
created_by:     "cosine_supports"   (matches source — ADR-001)
created_at:     current_timestamp_secs()
metadata:       '{"cosine": <f32>}'   (resolved decision 4)
bootstrap_only: 0
```

---

## Sequencing Constraints (Build Order)

1. `unimatrix-store` must compile with `EDGE_SOURCE_COSINE_SUPPORTS` before
   `unimatrix-server` imports it.
2. `InferenceConfig.supports_cosine_threshold` must be present before
   `nli_detection_tick.rs` compiles, because the tick reads it from `config`.
3. `write_graph_edge` must be defined in `nli_detection.rs` before
   `nli_detection_tick.rs` calls it.
4. The `use crate::services::nli_detection::{..., write_graph_edge}` import in
   `nli_detection_tick.rs` must be extended to include `write_graph_edge`.
5. `use unimatrix_store::EDGE_SOURCE_COSINE_SUPPORTS` (or the existing wildcard import
   if `unimatrix_store::*` is already in scope in `nli_detection_tick.rs`) must be
   verified at the top of the tick file.

---

## Key Invariants

| Invariant | Source |
|-----------|--------|
| `write_nli_edge` must NOT be modified | SPECIFICATION.md FR-12, WARN-04 |
| `write_graph_edge` returns `rows_affected() > 0`: `true`=inserted, `false`=UNIQUE conflict or SQL error | R-07, ADR-001 |
| UNIQUE conflict (`false`, `rows_affected=0`) is NOT an error — no `warn!` at call site | R-07, ADR-001 |
| SQL error (`false`, `Err`) emits `warn!` inside `write_graph_edge` — caller must NOT double-log | R-07 |
| Budget counter incremented only on `true` return | RISK-TEST-STRATEGY failure modes |
| Observability log fires unconditionally after loop | WARN-02, ADR-003, R-06 |
| No per-pair DB lookup in Path C loop | WARN-01, NFR-09 |
| `run_graph_inference_tick` remains infallible (`-> ()`) | FR-15, SR-07 |
| Dual-site default for `supports_cosine_threshold` (serde fn + impl Default) | ADR-002 |
| All 6 sites of `nli_post_store_k` removed | FR-09, AC-17 |
| `inferred_edge_count` continues counting only `source='nli'` | NFR-06, backward compat |
