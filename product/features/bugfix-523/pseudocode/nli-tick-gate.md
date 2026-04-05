# Pseudocode: nli-tick-gate (Item 1)

## Purpose

Insert an explicit `nli_enabled` gate inside `run_graph_inference_tick` at the PATH B entry
boundary. This eliminates the latent 353-second tick congestion caused by the implicit
gate (awaiting `get_provider()` which returns Err when NLI is disabled) and makes operator
intent observable via a distinct log message.

## File

`crates/unimatrix-server/src/services/nli_detection_tick.rs`

## Scope

One insertion block (3 lines of Rust) plus a comment update at the `get_provider()` call site.
No logic changes elsewhere in the function.

---

## Modified Function: `run_graph_inference_tick`

### Existing Structural Sequence at PATH B Entry (lines 544–568, verified from source)

```
// [line 544] .await;    ← run_cosine_supports_path completes (Path C done)
//
// [line 546] // === PATH B entry gate ===
// [line 547] // Informs writes (Path A) are complete above. Path C (cosine Supports) also complete.
// [line 548] // Path B gates NLI Supports only.
//
// [line 552] if candidate_pairs.is_empty() {
// [line 553]     tracing::debug!("graph inference tick: no Supports candidates; skipping NLI batch");
// [line 554]     return;
// [line 555] }
//
//            ← [INSERTION POINT for Item 1 gate — between line 555 and 560]
//
// [line 557] // R-01 CRITICAL: get_provider() is the SOLE entry point to Phase 6/7/8.
// [line 558] // Err return here structurally prevents ANY Phase 8 write without a successful provider.
// [line 559] // No code path from get_provider() Err to write_nli_edge for Supports edges exists.
// [line 560] let provider = match nli_handle.get_provider().await {
// [line 562]     Ok(p) => p,
// [line 563]     Err(_) => {
// [line 564]         // Expected when nli_enabled=false (production default).  ← MUST UPDATE
// [line 565]         tracing::debug!("graph inference tick: NLI provider not ready; Supports path skipped");
// [line 566]         return;
// [line 567]     }
// [line 568] };
```

### After Change: PATH B Entry Sequence

```
// [unchanged] run_cosine_supports_path(...).await;  ← Path C completes

// [unchanged] // === PATH B entry gate ===
// [unchanged] // Informs writes (Path A) are complete above. Path C (cosine Supports) also complete.
// [unchanged] // Path B gates NLI Supports only.

// [unchanged] Fast exit: no Supports candidates
if candidate_pairs.is_empty() {
    tracing::debug!("graph inference tick: no Supports candidates; skipping NLI batch");
    return;
}

// [NEW] Explicit nli_enabled gate — must be AFTER candidate_pairs.is_empty() check
// and BEFORE get_provider().await to avoid the async call when NLI is intentionally off.
// Message is intentionally distinct from the get_provider() Err message so operators
// can distinguish intentional-off (this message) vs. transient-not-ready (Err message).
if !config.nli_enabled {
    tracing::debug!("graph inference tick: NLI disabled by config; Path B skipped");
    return;
}

// [updated comment] get_provider() Err here is a TRANSIENT provider-not-ready condition.
// The nli_enabled=false case is now handled by the explicit gate above.
// R-01 CRITICAL: get_provider() is the SOLE entry point to Phase 6/7/8.
let provider = match nli_handle.get_provider().await {
    Ok(p) => p,
    Err(_) => {
        // Transient: provider not yet initialized or temporarily unavailable.
        tracing::debug!("graph inference tick: NLI provider not ready; Supports path skipped");
        return;
    }
};

// ... remainder of Phase 6/7/8 unchanged ...
```

### Debug Message (prescribed, exact text, must not be altered)

```
"graph inference tick: NLI disabled by config; Path B skipped"
```

### Invariants Preserved

- Phase A (Informs write loop, Phase 4b) executes unconditionally before this gate.
- Path C (`run_cosine_supports_path`) executes unconditionally before this gate.
- `background.rs` call site of `run_graph_inference_tick` is NOT modified (C-01).
- The `candidate_pairs.is_empty()` fast-exit remains before the `nli_enabled` gate.
- Gate fires only for Path B (get_provider, rayon dispatch, Phase 8 writes).

---

## Error Handling

This function has no return value (returns `()`). Both the new gate and the existing
`candidate_pairs.is_empty()` fast-exit are silent early returns logged at `debug!` level.
No error propagation — all failures within the tick are handled by logging and returning.

---

## Key Test Scenarios

### AC-01: Path B skipped when `nli_enabled=false` with non-empty candidates

```
GIVEN: InferenceConfig { nli_enabled: false, ... }
AND:   candidate_pairs is non-empty (at least one pair)
AND:   a mock NliServiceHandle in Ready state (provider would be available if called)
WHEN:  run_graph_inference_tick() is called
THEN:  no NLI Supports edges are written to the store
AND:   no rayon dispatch occurs (behavioral proxy: no NLI-sourced edges present)
AND:   function returns without panic

NOTE:  candidate_pairs must be non-empty to reach the nli_enabled check.
       The empty-candidates fast-exit fires before the nli_enabled gate, so
       an empty pair list would not exercise the new gate.
```

Test function name: `test_nli_gate_path_b_skipped_nli_disabled`

### AC-02a: Path A (Informs) still runs when `nli_enabled=false`

```
GIVEN: InferenceConfig { nli_enabled: false, ... }
AND:   candidate pairs that qualify for Informs edges (cosine >= nli_informs_cosine_floor,
       valid category pair)
WHEN:  run_graph_inference_tick() is called
THEN:  Informs edges are present in the graph store
AND:   function returns without panic
```

Test function name: `test_nli_gate_path_a_informs_edges_still_written_nli_disabled`

### AC-02b: Path C (cosine Supports) still runs when `nli_enabled=false`

```
GIVEN: InferenceConfig { nli_enabled: false, ... }
AND:   candidate pairs that pass supports_cosine_threshold
WHEN:  run_graph_inference_tick() is called
THEN:  cosine Supports edges are written by run_cosine_supports_path
AND:   function returns without panic
```

Test function name: `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled`

### AC-03: NLI-enabled path not regressed

```
GIVEN: InferenceConfig { nli_enabled: true, ... }
AND:   a mock NliServiceHandle in Ready state (get_provider() returns Ok)
AND:   non-empty candidate_pairs
WHEN:  run_graph_inference_tick() is called
THEN:  get_provider() is called (gate does not fire)
AND:   Phase 6/7/8 execute (behavioral proxy: NLI Supports edge present or rayon dispatch occurs)
```

Test function name: `test_nli_gate_nli_enabled_path_not_regressed`

---

## Risks Addressed

- R-01/R-02: Gate placement is verified by AC-02b — if gate were before `run_cosine_supports_path`,
  Path C would produce no edges, and `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled`
  would fail.
- R-09: AC-03 confirms the enabled path is not broken by the new guard condition.

## Knowledge Stewardship

- Pattern #3675 (tick loop gate patterns): confirms the gate belongs inside the function,
  not at the call site.
- ADR-001 (entry #4017, crt-039): structural invariant — Path A and Path C unconditional.
  This pseudocode preserves that invariant by inserting the gate after both paths complete.
- ADR-001 (entry #4143, bugfix-523): gate placement and distinct log message prescribed.
- Deviations from established patterns: none.
