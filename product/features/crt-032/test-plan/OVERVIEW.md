# Test Plan Overview: crt-032 — w_coac Reduction to 0.0

## Test Strategy

This feature is a pure default-value change with cascading doc comment and test assertion updates.
No new logic, no schema changes, no new MCP tool surface.

**Delivery gate**: `cargo test --workspace` pass. No eval re-run, no new integration tests.

### Test Layers

| Layer | What | Gate |
|-------|------|------|
| Unit: default-path | Existing tests that verify compiled defaults | Must pass after assertion update |
| Unit: fixture-path | Tests using `w_coac: 0.10` as explicit fixture | Must be unchanged and still pass |
| Unit: sum invariant | Sum upper-bound tests | Pass naturally at 0.85 ≤ 0.95 |
| Integration: smoke | infra-001 smoke suite (mandatory minimum) | Must pass |

---

## Risk-to-Test Mapping

| Risk | Priority | Test(s) | Change Required |
|------|----------|---------|-----------------|
| R-01: Inconsistent default (dual sites) | Critical | `test_inference_config_weight_defaults_when_absent` (serde path) + `test_inference_config_default_weights_sum_within_headroom` (Default::default() path) | Update assertion in first test |
| R-02: Default-assertion test left at 0.10 | Critical | `test_inference_config_weight_defaults_when_absent` | Update assertion + message |
| R-03: Fixture tests incorrectly changed | High | `test_inference_config_validate_accepts_sum_exactly_one` + search.rs `FusionWeights` fixtures | Verify unchanged post-delivery |
| R-04: Stale doc comments | Medium | Grep assertions (4 sites) | Verify no `0.95` or `Default: 0.10` remain |
| R-05: CO_ACCESS_STALENESS_SECONDS modified | Medium | Grep + read constant | Verify value and 3 call sites unchanged |
| R-06: compute_search_boost/briefing_boost removed | Medium | Grep for function definitions | Verify both present |
| R-07: Partial-TOML comment not updated | Low | `test_inference_config_partial_toml_gets_defaults_not_error` comment | Verify comment updated to 0.0/0.90 |

---

## Integration Harness Plan

### Suite Selection

This feature touches:
- Internal config defaults only — no MCP tool behavior changes
- No scoring logic changes (w_coac multiplied by 0.0 == 0.0, but the path was already exercised)

Per the suite selection table: "Any change at all → `smoke` (minimum gate)"

**Suites to run**: smoke only (`pytest -m smoke --timeout=60`)

**No new integration tests needed**: This change has no MCP-visible behavioral effect through the tool interface. The scoring path with `w_coac=0.0` produces results identical to before when operators use the default. No integration test can distinguish a 0.0-multiplied boost from a 0.0 boost — both produce zero contribution to the fused score. Unit tests are the appropriate coverage.

### Integration Test Verification

After Stage 3b implementation:
1. Run: `cd /workspaces/unimatrix/product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60`
2. Expected: all smoke tests pass (no feature-related failures)
3. Any failures: triage per USAGE-PROTOCOL.md — this feature cannot cause new smoke failures

---

## Non-Negotiable Tests

These must exist and pass (not xfail'd) after delivery:

| Test | File | Non-Negotiable Because |
|------|------|------------------------|
| `test_inference_config_weight_defaults_when_absent` | config.rs | Verifies both default paths post-change |
| `test_inference_config_default_weights_sum_within_headroom` | config.rs | Verifies sum invariant |
| `test_inference_config_validate_accepts_sum_exactly_one` | config.rs | Must remain with `w_coac: 0.10` as fixture |
| `test_inference_config_validate_rejects_w_coac_below_zero` | config.rs | Verifies field validation still active |
| `test_inference_config_partial_toml_gets_defaults_not_error` | config.rs | Verifies partial TOML uses new default |

---

## Cross-Cutting Notes

- No new test functions need to be written. All risks are covered by existing tests after assertion updates.
- Integration test count before vs after delivery should be identical.
- The search.rs FusionWeights fixture count (`w_coac: 0.10`) must be identical before vs after delivery.
