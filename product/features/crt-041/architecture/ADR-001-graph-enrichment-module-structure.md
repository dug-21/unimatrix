## ADR-001: Graph Enrichment Module Structure and Tick Placement

### Context

Three new edge signal sources (S1 tag co-occurrence, S2 structural vocabulary,
S8 search co-retrieval) must be integrated into the background tick pipeline.
The question is where to define the functions and where to call them.

Two placement options for S1/S2 were considered:

**Option A** — Extend `nli_detection_tick.rs` (rename: `graph_inference_tick.rs`)
to include S1/S2 phases inside `run_graph_inference_tick`. Co-locates all
graph-write logic. `nli_detection_tick.rs` is already >2,000 lines, well beyond
the 500-line workspace rule.

**Option B** — New module `graph_enrichment_tick.rs` called from `run_single_tick`
after `run_graph_inference_tick`. S1, S2, S8 are structurally distinct (pure SQL,
no HNSW, no NLI model) and belong in their own file. Avoids making an already-large
module worse. Follows the precedent of `co_access_promotion_tick.rs` as a standalone
module for SQL-only graph writes.

For S8, both options agree: S8 is batched (every N ticks), writes CoAccess edges
(not Informs), reads `audit_log` — structurally different from the NLI path. It
belongs in the same `graph_enrichment_tick.rs` as S1/S2 for cohesion (all three
are bulk graph enrichment), not in `nli_detection_tick.rs`.

The `write_graph_edge` function (crt-040 ADR-001) in `nli_detection.rs` is the
shared edge writer that S1/S2/S8 all call. It is NOT defined in
`graph_enrichment_tick.rs` — it lives in `nli_detection.rs` where it was introduced
as the generalized sibling of `write_nli_edge`.

**crt-040 prerequisite gate:** Before writing any S1/S2/S8 call sites, the delivery
agent must verify `write_graph_edge` exists:
```
grep -n "pub(crate) async fn write_graph_edge" \
    crates/unimatrix-server/src/services/nli_detection.rs
```
If absent, the delivery agent adds it as the first implementation step, with
`write_nli_edge` delegating to it (per crt-040 ADR-001 decision). This is not
optional — using `write_nli_edge` for S1/S2/S8 would silently tag those edges
with `source='nli'`, corrupting source-based filtering for GNN feature construction.

### Decision

Create a new module `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`
containing three `pub(crate) async fn` tick functions:

- `run_s1_tick(store: &Store, config: &InferenceConfig)` — S1 tag co-occurrence
- `run_s2_tick(store: &Store, config: &InferenceConfig)` — S2 structural vocabulary
- `run_s8_tick(store: &Store, config: &InferenceConfig)` — S8 search co-retrieval

Module constant (private): `const S8_WATERMARK_KEY: &str = "s8_audit_log_watermark"`

Register in `services/mod.rs`. Call from `background.rs::run_single_tick` after
`run_graph_inference_tick` in this order:
1. `run_s1_tick(store, config).await` — always
2. `run_s2_tick(store, config).await` — always (no-op when `s2_vocabulary` is empty)
3. `run_s8_tick(store, config).await` — gated: `current_tick % config.s8_batch_interval_ticks == 0`

Update the tick-ordering invariant comment in `background.rs` to include
`graph_enrichment_tick` after `structural_graph_tick`.

If the module exceeds 500 lines excluding tests, extract tests to a
`graph_enrichment_tick_tests.rs` sibling with `#[cfg(test)] #[path = "..."] mod tests`.

All three functions follow the `co_access_promotion_tick` design contract:
- `async fn ... -> ()` (infallible)
- All SQL errors logged at `tracing::warn!`, tick continues
- Always emits `tracing::info!` with edges-written and edges-skipped counts on completion
- Uses `store.write_pool_server()` directly — no analytics queue, no rayon, no spawn_blocking

### Consequences

Easier: `graph_enrichment_tick.rs` has a single responsibility (bulk SQL graph enrichment).
Adding a fourth source (e.g., S3, S4) is a new function in this file, not a change to
`nli_detection_tick.rs`. The 500-line limit is enforceable from the start.

Harder: Three call sites in `background.rs` instead of one. The crt-040 prerequisite
gate is a real dependency — if crt-040 ships without `write_graph_edge`, crt-041
delivery must add it first, adding a small unscoped task. Both risks are low; the
prerequisite is explicitly gated and the call sites are trivial.
