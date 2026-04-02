## ADR-003: Path C Placement — After Path A, Before Path B Gate

### Context

`run_graph_inference_tick` has two existing paths after Phase 5 caps are applied:

- **Path A** (unconditional): writes `Informs` edges from `informs_metadata`, then
  emits the observability log. Runs on every tick regardless of NLI availability.
- **Path B** (NLI-gated): entered only when `get_provider()` succeeds. Phases 6/7/8
  write `Supports` edges. Currently inactive in production (`nli_enabled=false` default).

Path C is a new write loop that reads from `candidate_pairs` (Phase 4 output, already
computed for Path B candidates). Three placement options exist:

**Option 1 — Before Path A:** Path C runs first, writing Supports edges, then Path A
writes Informs edges. Ordering issue: `existing_supports_pairs` (Phase 2) is the tick-start
snapshot. Intra-tick Path C writes are not reflected in it. However, this is true for any
placement since we do not re-query the DB mid-tick. No correctness difference between
before/after Path A for the pre-filter.

**Option 2 — After Path A, Before Path B gate:** Path C runs after the Path A loop and
its observability log but before the `get_provider()` check that gates Phase 6/7/8.
The early-return at the Path B gate (`if candidate_pairs.is_empty() { return; }`)
currently short-circuits before `get_provider()`. With Path C inserted before this gate,
Path C runs even when `candidate_pairs` is empty — but an empty `candidate_pairs` means
Path C's loop is a no-op. This is acceptable.

**Option 3 — Alongside Path B (interleaved):** Path C runs inside Phase 8 alongside the
NLI write loop. This would entangle cosine Supports with NLI Supports in one write loop,
requiring branching on signal type. This increases complexity and risk of mis-tagging.

Option 2 has the clearest structural separation: Path A (structural Informs) completes,
then Path C (structural Supports) completes, then Path B (NLI Supports) is conditionally
entered. This matches the progression from "unconditional structural paths" to "conditional
model-dependent paths" which is the design intent of crt-039's path split.

Regarding SR-09 (pre-filter staleness): Path C runs before Path B. When `nli_enabled=true`,
Path B writes Supports edges after Path C. The pre-filter `existing_supports_pairs` was
built at Phase 2 (tick start). Any Path C writes are not in the pre-filter when Path B
runs — but `INSERT OR IGNORE` on `UNIQUE(source_id, target_id, relation_type)` is the
authoritative dedup. Path B's second insert for the same typed pair is silently ignored.
Likewise, any Path B writes would not be in the pre-filter when Path C checks it, but
Path C runs before Path B in Option 2, so this direction never occurs.

The early-return guard `if candidate_pairs.is_empty() { return; }` currently sits at the
Path B entry gate. With Path C inserted before it, the guard must be re-evaluated: Path C
should still run (as a no-op) even when `candidate_pairs` is empty, but the Path B fast
exit for an empty `candidate_pairs` remains valid. The guard placement moves to after
Path C, or is augmented with a separate check that also tests whether Path C produced
any writes (for observability).

### Decision

Path C is inserted after the Path A observability log and before the existing Path B
entry guard. The tick structure becomes:

```
[Phase 5: caps applied]
=== PATH A: Informs write loop ===
  for candidate in informs_metadata: write_graph_edge(source="nli", rel="Informs")
[Path A observability log]
=== PATH C: Cosine Supports write loop (NEW) ===
  for (src, tgt, cosine) in candidate_pairs:
    if cosine >= threshold AND category_pair matches AND not deduped:
      write_graph_edge(source="cosine_supports", rel="Supports")
[Path C observability log]
=== PATH B entry gate ===
  if candidate_pairs.is_empty() { return; }   // fast exit still valid after Path C
  let provider = match nli_handle.get_provider() ...
[Phase 6/7/8: NLI Supports unchanged]
```

The existing early-return `if candidate_pairs.is_empty() { return; }` remains at the
Path B gate position and continues to short-circuit Phases 6/7/8 when there are no NLI
candidates. Path C above it may have iterated `candidate_pairs` and found zero matches
(if cosine threshold was not met) or produced writes — both outcomes are valid and Path B
fast-exit is still correct.

### Consequences

**Easier:**
- Path C is structurally separate from Path A and Path B. The module doc comment
  block gains a `# Path C: Cosine Supports` section alongside the existing Path A and
  Path B sections.
- `nli_enabled=false` (production default) leaves Path C fully active. Path C does not
  depend on `get_provider()` or any model-loaded state.
- SR-09 (pre-filter staleness when Path B writes in same tick) is resolved by
  `INSERT OR IGNORE` dedup — documented in ARCHITECTURE.md.
- The tick function signature is unchanged; no new parameters needed.

**Harder:**
- The Path A observability log currently uses a return-path `tracing::debug!` that
  counts candidates/written. A parallel log for Path C must be added. The metric names
  must not collide with Path A's field names in the structured log (e.g.,
  `cosine_supports_candidates`, `cosine_supports_edges_written`).
- `nli_detection_tick.rs` gains another write loop. Delivery must evaluate whether to
  extract Path C into a private helper function to keep the tick function's line count
  manageable (SCOPE.md 500-line constraint).
