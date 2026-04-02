# Test Plan: background.rs — Tick Orchestrator

Component: `crates/unimatrix-server/src/services/background.rs`
Pseudocode: `product/features/crt-039/pseudocode/background.md`

---

## What Changes

1. The `if inference_config.nli_enabled { run_graph_inference_tick(...) }` outer gate in
   `run_single_tick` is removed. The call becomes unconditional.
2. The contradiction scan block gains a named section comment making its independence explicit.
3. The tick ordering invariant comment is added or updated.

These are structural changes only — no new logic, no new functions.

---

## Testability Assessment

`background.rs` changes are not directly unit-testable because:
- `run_single_tick` requires a full server context (Store, VectorIndex, NliServiceHandle,
  RayonPool, config, tick counter)
- The outer gate removal has no observable return value — it is a control flow change

The behavioral consequence of removing the outer gate IS testable: after removal,
`run_graph_inference_tick` runs even when `nli_enabled=false`, so Informs edges accumulate.
TC-01 in `nli_detection_tick.rs` validates this consequence at the inner function level.

---

## Unit Test Expectations

No new unit tests in `background.rs`. Verification is by code inspection.

**AC-01 — Gate removal (code inspection)**
```
ASSERT: run_single_tick does not contain:
  if inference_config.nli_enabled { ... run_graph_inference_tick ... }
ASSERT: run_graph_inference_tick is called unconditionally
VERIFICATION: Read background.rs, confirm no nli_enabled condition wraps the call
```

**AC-07 — Ordering invariant (code inspection)**
```
ASSERT: Ordering invariant comment is present in run_single_tick:
  // Tick ordering invariant (non-negotiable):
  // compaction → promotion → graph-rebuild
  //   → contradiction_scan (if embed adapter ready, every CONTRADICTION_SCAN_INTERVAL_TICKS)
  //   → extraction_tick → structural_graph_tick (always)
ASSERT: Call sequence matches the comment
```

**AC-06 — Contradiction scan zero-diff (diff inspection)**
```
ASSERT: git diff on contradiction scan block shows:
  - Only line additions (comment lines)
  - No condition mutations
  - No bracket reordering
  - Same && operator structure in condition
VERIFICATION: git diff background.rs | grep '^-' must not touch the scan condition line
```

---

## Integration Test Expectations

**TC-01 (in nli_detection_tick.rs)** is the integration-level validation that the outer
gate removal had the intended effect: Phase 4b runs and writes Informs edges.

If the outer gate is NOT removed from `background.rs`, TC-01 would need to call
`run_graph_inference_tick` directly (bypassing background.rs) — but TC-01 may test at
the inner function level anyway, so this risk needs code inspection to confirm.

**R-11 — Tick ordering invariant** is also validated by any existing integration test
that exercises a full tick cycle (e.g., `test_tick_liveness` in infra-001 availability
suite). These are not new tests — their continued passage is the regression check.

---

## Risk Coverage for This Component

| Risk | Mitigation | Verification |
|------|------------|-------------|
| R-09: Contradiction scan behavioral change | NFR-07 zero-diff constraint; existing tests pass | Diff audit + existing test regression |
| R-11: Tick ordering disturbed | Code ordering invariant comment; existing tick tests | Code inspection + existing test regression |

---

## Edge Cases (No Test Required — Code Inspection Sufficient)

- `nli_enabled = true` with NLI provider unavailable: `run_graph_inference_tick` is now called
  unconditionally, and the inner `get_provider()` guard handles this case. The outer gate
  previously would have short-circuited; now it reaches the inner gate instead. Behavior
  is equivalent for the Supports path. TC-02 verifies zero Supports edges in this scenario.
- `nli_enabled = false`: Previously the entire function was skipped. After crt-039, Phase 4b
  runs. This is the intended behavior change — TC-01 validates it.
