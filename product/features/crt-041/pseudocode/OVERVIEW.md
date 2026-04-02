# crt-041 Pseudocode Overview — Graph Enrichment: S1, S2, S8

## Components Involved

| Component | File | Action |
|-----------|------|--------|
| `graph_enrichment_tick` | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | CREATE |
| `edge_constants` | `crates/unimatrix-store/src/read.rs` + `lib.rs` | MODIFY |
| `config` | `crates/unimatrix-server/src/infra/config.rs` | MODIFY |
| `background` | `crates/unimatrix-server/src/background.rs` | MODIFY |
| `services/mod.rs` | `crates/unimatrix-server/src/services/mod.rs` | MODIFY (register module) |

## Prerequisite Status (verified)

`write_graph_edge` exists in `crates/unimatrix-server/src/services/nli_detection.rs` (shipped in crt-040). The crt-040 prerequisite gate passes. No conditional add needed.

**SIGNATURE DEVIATION (critical — delivery agent must read this):**
The actual `write_graph_edge` signature differs from the IMPLEMENTATION-BRIEF spec:

```
// BRIEF specifies:
pub(crate) async fn write_graph_edge(
    store: &Store, source_id: u64, target_id: u64, relation_type: &str,
    weight: f64, created_at: u64, source: &str, metadata: Option<&str>,
) -> bool

// ACTUAL (crt-040 shipped):
pub(crate) async fn write_graph_edge(
    store: &Store, source_id: u64, target_id: u64, relation_type: &str,
    weight: f32, created_at: u64, source: &str, metadata: &str,
) -> bool
```

Differences:
- `weight` is `f32`, not `f64` — call sites must pass `weight as f32`
- `metadata` is `&str` (non-optional, empty string `""` for no metadata), not `Option<&str>`
- `created_by` is NOT a separate parameter — the SQL binds `source` to BOTH `?6` slots,
  so `created_by` always equals `source`. The per-source `created_by` values in the brief
  ('s1', 's2', 's8') are irrelevant — the column will contain 'S1', 'S2', 'S8' respectively.
  This is acceptable: the `source` column is the authoritative discriminator for GNN features.

All call sites in `graph_enrichment_tick.rs` must use the actual signature.

## Data Flow Between Components

```
background.rs::run_single_tick
    │
    │ (reads)  InferenceConfig {s2_vocabulary, max_s1_edges_per_tick,
    │                           max_s2_edges_per_tick, s8_batch_interval_ticks,
    │                           max_s8_pairs_per_batch}
    │
    ├── run_graph_inference_tick(store, ...)          [existing — runs before]
    │
    └── run_graph_enrichment_tick(store, config, current_tick)   [NEW]
            │
            ├── run_s1_tick(store, config)
            │       reads:  entry_tags, entries (status=0 guard)
            │       writes: graph_edges (source='S1', relation_type='Informs')
            │       uses:   EDGE_SOURCE_S1 constant
            │               write_graph_edge (nli_detection.rs)
            │
            ├── run_s2_tick(store, config)
            │       reads:  entries (content+title, status=0 guard)
            │       writes: graph_edges (source='S2', relation_type='Informs')
            │       uses:   EDGE_SOURCE_S2 constant
            │               write_graph_edge (nli_detection.rs)
            │               sqlx::QueryBuilder (push_bind, never interpolation)
            │
            └── run_s8_tick(store, config)   [gated: tick % s8_batch_interval_ticks == 0]
                    reads:  counters (S8_WATERMARK_KEY), audit_log, entries (status=0 guard)
                    writes: graph_edges (source='S8', relation_type='CoAccess')
                            counters (updated watermark, AFTER all edge writes)
                    uses:   EDGE_SOURCE_S8 constant
                            write_graph_edge (nli_detection.rs)
                            serde_json (target_ids parsing)
                            counters::read_counter / counters::set_counter
```

## Shared Types (New or Modified)

### New constants in `unimatrix_store` (read.rs + lib.rs re-export)

```
pub const EDGE_SOURCE_S1: &str = "S1";
pub const EDGE_SOURCE_S2: &str = "S2";
pub const EDGE_SOURCE_S8: &str = "S8";
```

### New `InferenceConfig` fields (config.rs)

```
s2_vocabulary:           Vec<String>  default: vec![]
max_s1_edges_per_tick:   usize        default: 200    range: [1, 10000]
max_s2_edges_per_tick:   usize        default: 200    range: [1, 10000]
s8_batch_interval_ticks: u32          default: 10     range: [1, 1000]
max_s8_pairs_per_batch:  usize        default: 500    range: [1, 10000]
```

### Module constant in `graph_enrichment_tick.rs`

```
const S8_WATERMARK_KEY: &str = "s8_audit_log_watermark";
```

## Tick Ordering After crt-041

```
compaction
→ co_access_promotion
→ TypedGraphState::rebuild
→ PhaseFreqTable::rebuild
→ contradiction_scan (if embed adapter ready && tick_multiple)
→ extraction_tick
→ run_graph_inference_tick (structural_graph_tick — always)
→ run_graph_enrichment_tick (always)     ← NEW (after run_graph_inference_tick)
    → run_s1_tick                        ← always
    → run_s2_tick                        ← always (no-op when s2_vocabulary empty)
    → run_s8_tick                        ← gated: current_tick % s8_batch_interval_ticks == 0
```

New edges from this tick are visible to PPR at the NEXT tick's TypedGraphState::rebuild
(one-tick delay, same as co_access_promotion). Eval gate must run after at least one
full tick post-delivery.

## Sequencing Constraints

1. `edge_constants` must be defined and re-exported before `graph_enrichment_tick.rs` compiles.
2. `config` fields must be added before `background.rs` compiles (InferenceConfig is passed by reference).
3. `services/mod.rs` must register `graph_enrichment_tick` before `background.rs` import resolves.
4. `write_graph_edge` is already present — no prerequisite work needed.

## Key Invariants Embedded in Pseudocode

- C-03: dual-endpoint quarantine guard (`status = 0` JOIN on both source and target)
- C-05: S2 vocabulary via `push_bind` only, never string format!()
- C-07: InferenceConfig dual-site maintenance (default_*() fn AND impl Default literal)
- C-08: validate() lower-bound 1 for all four numeric fields
- C-11: S8 watermark written AFTER all edge writes
- C-12: S8 cap on pairs, not on audit_log rows
- C-13: S8 quarantine filter chunked (<=999 IDs per IN clause)
- C-14: S8 malformed JSON advances watermark past that row (no stuck watermark)
