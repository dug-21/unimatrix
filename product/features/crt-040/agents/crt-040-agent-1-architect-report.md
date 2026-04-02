# Agent Report: crt-040-agent-1-architect

## Task
Design the architecture for crt-040 (Cosine Supports Edge Detection).

## Deliverables

### Files Produced
- `/workspaces/unimatrix/product/features/crt-040/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-040/architecture/ADR-001-edge-writer-generalization.md`
- `/workspaces/unimatrix/product/features/crt-040/architecture/ADR-002-impl-default-dual-site-requirement.md`
- `/workspaces/unimatrix/product/features/crt-040/architecture/ADR-003-path-c-placement-in-tick.md`
- `/workspaces/unimatrix/product/features/crt-040/architecture/ADR-004-cosine-supports-budget-constant.md`

### ADRs Stored in Unimatrix

| ADR | Title | Unimatrix ID |
|-----|-------|--------------|
| ADR-001 | Generalized Edge Writer — write_graph_edge Sibling | #4027 |
| ADR-002 | InferenceConfig supports_cosine_threshold Dual-Site Default | #4028 |
| ADR-003 | Path C Placement — After Path A, Before Path B Gate | #4029 |
| ADR-004 | Cosine Supports Budget — MAX_COSINE_SUPPORTS_PER_TICK = 50 Constant | #4030 |

## Key Decisions

1. **Edge writer generalization (ADR-001, #4027):** New `write_graph_edge(store, src, tgt,
   rel, weight, ts, source, meta) -> bool` pub(crate) function in `nli_detection.rs`.
   `write_nli_edge` refactored to delegate to it with hardcoded `source="nli"`. All
   existing Path A and Path B callers unchanged.

2. **Config dual-site (ADR-002, #4028):** `supports_cosine_threshold: f32` must be set
   in BOTH the `#[serde(default = "default_supports_cosine_threshold")]` backing fn AND
   in the `impl Default` struct literal (as a call to the backing fn, not a repeated
   literal). Unit test required for `InferenceConfig::default().supports_cosine_threshold`.
   `nli_post_store_k` removed in same change.

3. **Path C placement (ADR-003, #4029):** After Path A observability log, before the
   Path B entry gate. Runs unconditionally on every tick regardless of `nli_enabled`.
   INSERT OR IGNORE dedup is authoritative when Path B and Path C both attempt the same
   pair — not treated as an error.

4. **Budget constant (ADR-004, #4030):** `MAX_COSINE_SUPPORTS_PER_TICK = 50` as a
   module constant in `nli_detection_tick.rs`, adjacent to `MAX_INFORMS_PER_TICK`.
   No InferenceConfig field.

## SR-04 UNIQUE Constraint Verification (CONFIRMED)

Inspected DDL at four sites: `db.rs`, `migration.rs` (v13 step), `analytics.rs`
(test helper), `read.rs` (test helper). All four are consistent:

```sql
UNIQUE(source_id, target_id, relation_type)
```

The `source` column is NOT in the constraint. Path B and Path C can both attempt a
`Supports` edge for the same pair; `INSERT OR IGNORE` correctly discards the second
insert. No migration needed, no DDL change needed.

## Integration Surface (Exact Contracts for Delivery)

| Symbol | Signature / Value | File |
|--------|-------------------|------|
| `write_graph_edge` | `async fn(store: &Store, source_id: u64, target_id: u64, relation_type: &str, weight: f32, created_at: u64, source: &str, metadata: &str) -> bool` | `nli_detection.rs` (new) |
| `EDGE_SOURCE_COSINE_SUPPORTS` | `&str = "cosine_supports"` | `unimatrix-store/src/read.rs` (new) |
| `supports_cosine_threshold` | `f32`, default `0.65`, range `(0.0, 1.0)` exclusive | `infra/config.rs` (new field) |
| `MAX_COSINE_SUPPORTS_PER_TICK` | `usize = 50` | `nli_detection_tick.rs` (new constant) |

## Open Questions for Follow-up Issues

1. `informs_category_pairs` reuse as Supports filter — separate `supports_category_pairs`
   field needed if same-category Supports are added in a future feature (SR-05).
2. `MAX_COSINE_SUPPORTS_PER_TICK` is not operator-tunable; config promotion path flagged
   via TODO comment at constant (SR-03).
3. `inferred_edge_count` in `GraphCohesionMetrics` counts only `source='nli'` — stale
   name; follow-up issue for adding `cosine_supports_edge_count` or renaming.
