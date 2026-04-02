# Agent Report: crt-041-agent-1-architect

## Task
Design architecture for crt-041 — Graph Enrichment S1, S2, S8 Edge Sources.

## Artifacts Produced

### Architecture
- `/workspaces/unimatrix/product/features/crt-041/architecture/ARCHITECTURE.md`

### ADR Files
- `/workspaces/unimatrix/product/features/crt-041/architecture/ADR-001-graph-enrichment-module-structure.md`
- `/workspaces/unimatrix/product/features/crt-041/architecture/ADR-002-s2-safe-sql-construction.md`
- `/workspaces/unimatrix/product/features/crt-041/architecture/ADR-003-s8-watermark-strategy.md`
- `/workspaces/unimatrix/product/features/crt-041/architecture/ADR-004-graphcohesionmetrics-extension.md`
- `/workspaces/unimatrix/product/features/crt-041/architecture/ADR-005-inferenceconfig-dual-maintenance-guard.md`

## ADR Unimatrix Entry IDs

| ADR | Title | Entry ID |
|-----|-------|----------|
| ADR-001 | Graph Enrichment Module Structure and Tick Placement | #4031 |
| ADR-002 | S2 Safe SQL Construction via sqlx::QueryBuilder | #4032 |
| ADR-003 | S8 Watermark Strategy — Counters Table, Write-After-Commit Order | #4033 |
| ADR-004 | GraphCohesionMetrics Extension Scope — No Changes Needed | #4034 |
| ADR-005 | InferenceConfig Dual-Maintenance Guard for Five New Fields | #4035 |

## Key Design Decisions

1. **New module `graph_enrichment_tick.rs`** with `run_s1_tick`, `run_s2_tick`,
   `run_s8_tick` — all `pub(crate) async fn(store, config)`, infallible, no rayon.
   Called from `background.rs::run_single_tick` after `run_graph_inference_tick`.

2. **crt-040 prerequisite gate** — delivery agent must verify `write_graph_edge`
   exists in `nli_detection.rs` before writing any S1/S2/S8 call sites. If absent,
   add it as the first delivery step.

3. **S2 SQL injection prevention** — `sqlx::QueryBuilder::push_bind` for all
   vocabulary terms. No term is ever part of the SQL string. Early return on empty
   vocabulary (AC-06).

4. **S8 watermark** — `counters` table key `s8_audit_log_watermark`. Write AFTER
   edge inserts. Malformed JSON rows advance the watermark (do not stall the batch).
   Bulk quarantine pre-fetch instead of per-pair round-trips.

5. **GraphCohesionMetrics — no changes needed** — `cross_category_edge_count` and
   `isolated_entry_count` already exist from col-029. The eval gate uses existing fields.

6. **InferenceConfig dual-site invariant** — five new fields, each with matching
   `default_*()` serde function AND `impl Default` struct literal value. `s2_vocabulary`
   defaults to empty (operator opt-in, domain-agnostic). All four range-bounded fields
   get `validate()` checks with lower bound 1 (not 0) to guard SQL LIMIT usage.
   Pre-delivery test asserts serde path == Default path for all five fields.

## Integration Surface Summary

| Component | What Changes |
|-----------|-------------|
| `graph_enrichment_tick.rs` | New file — 3 tick functions |
| `nli_detection.rs` | `write_graph_edge` must exist (crt-040); verify before use |
| `read.rs` | 3 new constants: `EDGE_SOURCE_S1/S2/S8` |
| `lib.rs` | Re-export the 3 new constants |
| `config.rs` | 5 new `InferenceConfig` fields + serde fns + validate + Default + merge |
| `background.rs` | 3 new call sites after `run_graph_inference_tick`; updated ordering comment |
| `services/mod.rs` | Register new module |

## Open Questions
None. All design decisions resolved. The only conditionality is SR-04 (crt-040 prerequisite) — handled by explicit gate in ADR-001 and the delivery pre-flight check.
