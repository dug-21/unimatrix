# Test Plan: Background Call Site (crt-029)

Source file: `crates/unimatrix-server/src/services/background.rs`
Pseudocode: `pseudocode/background-call-site.md`

Risks addressed: R-11 (pub(crate) promotion compile gate), AC-14 (call site ordering and gate)

---

## What This Component Does

A single new call is added to `run_single_tick` in `background.rs`:

```rust
// crt-029: Background graph inference (recurring, cap-throttled via max_graph_inference_per_tick).
if inference_config.nli_enabled {
    run_graph_inference_tick(store, nli_handle, vector_index, ml_inference_pool, inference_config).await;
}
```

The call must be placed **after** `maybe_run_bootstrap_promotion`, not before. No new parameters
are added to `spawn_background_tick` or `background_tick_loop`. The `vector_index` reference
is already threaded through to `run_single_tick`; no signature changes are needed there.

---

## Testing Approach

`background.rs` wires together existing services. Its behaviour is best verified by:
1. A **code review check** — correct ordering and gate condition
2. **Integration tests** — observable effects through tick lifecycle (in infra-001 lifecycle suite)
3. **Compile gate** — missing `pub(crate)` promotions for `write_nli_edge`,
   `format_nli_metadata`, `current_timestamp_secs` in `nli_detection.rs` will fail here

Direct unit testing of `run_single_tick` is difficult (requires wiring up all service handles).
The priority is integration-level verification and the static checks below.

---

## Static Checks (Pre-Merge Gates)

### R-11 — `pub(crate)` promotions

```bash
grep -n 'pub(crate) fn write_nli_edge\|pub(crate) fn format_nli_metadata\|pub(crate) fn current_timestamp_secs' \
  crates/unimatrix-server/src/services/nli_detection.rs
```

Expected: all three present. These promotions must exist before `nli_detection_tick.rs` can
call them. Any missing promotion causes a compile error in `nli_detection_tick.rs`.

### AC-14 ordering — Call after bootstrap promotion

```bash
grep -n 'maybe_run_bootstrap_promotion\|run_graph_inference_tick' \
  crates/unimatrix-server/src/services/background.rs
```

Expected: `maybe_run_bootstrap_promotion` line number is lower than `run_graph_inference_tick`
line number — bootstrap runs before inference tick in source order.

### AC-14 gate — `nli_enabled` guard present

```bash
grep -n 'nli_enabled' crates/unimatrix-server/src/services/background.rs
```

Expected: the guard `if inference_config.nli_enabled` wraps the `run_graph_inference_tick`
call. The tick must not run unconditionally.

### Module declaration in `mod.rs`

```bash
grep -n 'nli_detection_tick' crates/unimatrix-server/src/services/mod.rs
```

Expected: `pub mod nli_detection_tick;` present. Missing declaration causes a compile error
referencing undefined module.

---

## Unit Test Expectations

### AC-14 — `nli_enabled = false` no-op

Where the test infrastructure allows constructing a mock `run_single_tick` context:

#### `test_background_tick_nli_disabled_skips_inference_tick`
- Setup: `InferenceConfig` with `nli_enabled = false`, plus mock service handles
- Run `run_single_tick` (or the tick loop body with mocks)
- Assert: `run_graph_inference_tick` is not called (verify via mock call counter or side effect)
- Assert: `maybe_run_bootstrap_promotion` IS still called (unchanged behaviour; tick is additive)

If the existing test infrastructure for `background.rs` does not support this level of mocking,
this test can be satisfied by the integration test `test_graph_inference_tick_nli_disabled`
in the infra-001 lifecycle suite (see OVERVIEW.md). Document in the Stage 3c report which
verification method was used.

### AC-14 — Call ordering invariant

The ordering is a code-structure property, not a runtime property. It cannot be tested
with a race condition detector or timing assertion. The grep gate above is the correct
verification method.

However, a meaningful ordering test:

#### `test_background_tick_ordering_bootstrap_before_inference`
- If the call site can be made observable via mock: assert that when both bootstrap promotion
  AND inference tick run, bootstrap's write completes before inference tick's Phase 2 reads
  begin (the inference tick's pre-filter query occurs after bootstrap's writes are committed)
- This is a medium-priority test; if not tractable with existing test infrastructure, document
  as "coverage provided by integration test + code review"

---

## Integration Harness Coverage

AC-14 is partially verified by infra-001 lifecycle suite tests:

- `test_graph_inference_tick_nli_disabled` — `nli_enabled = false` asserts tick not invoked
  (observable via zero increase in graph edge count over multiple ticks)
- `test_graph_inference_tick_writes_supports_edges` — `nli_enabled = true` asserts edges do
  appear after tick cycles (proves the call site fires when enabled)

These are the three new integration tests added in OVERVIEW.md.

---

## Integration Risks

### Call site ordering is not compile-enforced

There is no Rust mechanism that enforces `maybe_run_bootstrap_promotion` runs before
`run_graph_inference_tick`. If a future refactor swaps the order, the compiler will not
object. The grep gate is the only protection.

**Mitigation in test plan**: the grep gate above is mandatory. Additionally, a code comment
in `background.rs` next to the `run_graph_inference_tick` call should read:
"// Runs after maybe_run_bootstrap_promotion to see bootstrap-promoted edges in pre-filter."

### Phase 6 uses write pool (bootstrap promotion precedent)

`run_graph_inference_tick` Phase 6 uses `get_content_via_write_pool()` to see recently
committed rows. The tick's text fetch thus contends marginally with the Phase 8 edge writes.
This is documented in the risk strategy as benign at default cap (100 pairs). No unit test is
needed; the integration tests at scale (if run) would surface any serialization issue.

---

## Assertions Summary

| AC-ID | Test / Check | Expected Result |
|-------|-------------|-----------------|
| R-11 | grep: pub(crate) promotions in nli_detection.rs | All 3 present |
| AC-14 | grep: ordering in background.rs | bootstrap before inference (lower line number) |
| AC-14 | grep: nli_enabled guard in background.rs | Guard present |
| AC-14 | grep: mod declaration in mod.rs | `pub mod nli_detection_tick;` present |
| AC-14 | `test_background_tick_nli_disabled_skips_inference_tick` | Tick not called when disabled |
| AC-14 | infra-001: `test_graph_inference_tick_nli_disabled` | 0 inferred edges with nli_enabled=false |
| AC-14 | infra-001: `test_graph_inference_tick_writes_supports_edges` | Edges present with nli_enabled=true |
| (compile) | `cargo check -p unimatrix-server` | Passes with no errors |
