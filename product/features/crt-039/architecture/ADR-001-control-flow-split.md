## ADR-001: Control Flow Split in `run_graph_inference_tick`

### Context

`run_graph_inference_tick` opens with a Phase 1 guard:

```rust
let provider = match nli_handle.get_provider().await {
    Ok(p) => p,
    Err(_) => return,
};
```

This early return fires on every tick when `nli_enabled = false` (the production default),
blocking Phase 4b (structural Informs HNSW scan) which requires no NLI model at all.

Three structural options for decoupling were considered:

**Option X — Extract Phase 4b to a separate public function called first:**
`run_structural_informs_tick()` called unconditionally, then `run_nli_supports_tick()`
called conditionally. This produces two public functions in the same module and requires
the caller (`background.rs`) to orchestrate the sequence. It also splits the shared Phase 2
DB reads (active entries, isolated IDs, existing pairs) across two call sites, requiring
either a third shared function or duplication.

**Option Y — Result enum splitting call sites:**
`run_graph_inference_tick` returns an enum that records the Phase 4b output, which the
caller then optionally passes to a Phase 8 function. This pushes an internal phase-routing
concern into the caller.

**Option Z — Internal split within the single public function (chosen):**
Phase 4b and the Informs write loop execute first inside `run_graph_inference_tick`. The
`get_provider()` call moves to the Path B entry point — after Phase 4b completes and after
the Informs edges are written. If `get_provider()` returns `Err`, a conditional early
return at that point exits the function without touching Phase 6/7/8. Phase 4b has already
run by then.

Option Z is chosen because:
- The public function signature `run_graph_inference_tick(store, nli_handle, vector_index,
  rayon_pool, config)` is unchanged. No caller changes needed except removing the outer
  `nli_enabled` gate in `background.rs`.
- The Phase 2 shared DB reads (active entries, isolated IDs, existing pairs) run once and
  are consumed by both Path A and Path B — no duplication.
- The internal flow is self-documenting: readers see Path A (unconditional) and Path B
  (gated) as adjacent code blocks in a single function, not as two separate entry points.
- The crt-037 ADR-001 `NliCandidatePair` tagged union only serves the merged Phase 6/7/8
  pipeline. Path A never touches it. The union's `Informs` variant and `PairOrigin::Informs`
  are removed; the `SupportsContradict` variant is retained for Path B.

### Decision

`run_graph_inference_tick` is restructured as a single function with two sequential
execution paths:

**Path A (unconditional):** Phases 2–5 run for both Supports and Informs candidates.
The Informs write loop runs immediately after Phase 5, calling `apply_informs_composite_guard`
and `write_nli_edge` directly without any NLI provider. This path completes fully regardless
of NLI availability.

**Path B (conditional):** After Path A completes, if `candidate_pairs` (Supports candidates
from Phase 4) is empty, the function returns. Otherwise, `get_provider()` is called. On
`Err`, the function returns — no NLI batch, no Supports edge writes. On `Ok`, Phases 6–8
execute (text fetch, rayon NLI batch, Supports edge writes).

The SR-04 assertion — "if `get_provider()` returns Err, Phase 8 must not write any edges"
— is enforced by the control flow itself: the `get_provider()` call is the sole entry point
to Phases 6–8, and a conditional `return` on `Err` is the only path out that does not
reach Phase 8. There is no code path from `get_provider() Err` to `write_nli_edge` for
Supports edges. This is enforced structurally, not by assertion.

The outer `if inference_config.nli_enabled` gate in `background.rs` line 760 is removed.
`run_graph_inference_tick` is called unconditionally on every tick.

### Consequences

- The function signature is unchanged — zero caller impact beyond removing the outer gate.
- Phase 4b runs on every tick regardless of `nli_enabled`. Informs edges accumulate in
  production from tick 1 after deployment.
- SR-04 (Phase 8 silent data corruption) is structurally impossible: no code path reaches
  Phase 8 without a successful `get_provider()` call immediately above it.
- The `NliCandidatePair::Informs` and `PairOrigin::Informs` variants are dead code after
  the split and must be removed to avoid misleading future readers.
- The `test_run_graph_inference_tick_nli_not_ready_no_op` test semantics change: the test
  must be split into two targeted tests asserting Path A writes (Informs) and Path B
  does not write (Supports) when NLI is not ready.
