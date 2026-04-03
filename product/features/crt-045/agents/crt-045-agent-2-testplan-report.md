# Agent Report: crt-045-agent-2-testplan

**Phase:** Stage 3a — Test Plan Design
**Agent ID:** crt-045-agent-2-testplan
**Date:** 2026-04-03

---

## Outputs Produced

| File | Lines | Purpose |
|------|-------|---------|
| `product/features/crt-045/test-plan/OVERVIEW.md` | ~120 | Overall test strategy, risk-to-test mapping, integration harness plan |
| `product/features/crt-045/test-plan/EvalServiceLayer.md` | ~160 | Component test plan for `eval/profile/layer.rs` |
| `product/features/crt-045/test-plan/ppr-expander-enabled-toml.md` | ~150 | Component test plan for the TOML profile fix |
| `product/features/crt-045/test-plan/layer_tests.md` | ~190 | Component test plan for `eval/profile/layer_tests.rs` |

---

## Risk Coverage Mapping

| Risk ID | Priority | Coverage | Test(s) |
|---------|----------|----------|---------|
| R-01 | Med | Full | `test_from_profile_typed_graph_rebuilt_after_construction` Layer 1+3 |
| R-02 | Med | Full | `test_from_profile_typed_graph_rebuilt_after_construction` Layer 2+3 (three-layer ADR-003) |
| R-03 | **High** | Full | Fixture requirement: two Active entries + S1/S2/S8 edge, `bootstrap_only=0` |
| R-04 | Med | Full | `test_from_profile_returns_ok_on_cycle_error` |
| R-05 | Med | Full | `test_ppr_expander_enabled_profile_parses_cleanly` (unit, `eval/profile/tests.rs`) |
| R-06 | Low | Full | All existing `layer_tests.rs` and `eval/profile/tests.rs` tests pass unchanged |
| R-07 | Low | Residual | Accepted; sqlx query timeout is implicit guard; deferred per SPECIFICATION.md |
| R-08 | Low | Compiler | `pub(crate)` visibility enforced by Rust compiler; PR review gate |
| R-09 | Med | Manual | Pre-merge `unimatrix eval run --profile baseline.toml` MRR confirmation |
| R-10 | Low | Residual | Covered incidentally by AC-06 test; from_profile() is sequential |

---

## Non-Negotiable Tests (gate-blocking per RISK-TEST-STRATEGY.md)

1. `test_from_profile_typed_graph_rebuilt_after_construction` — three-layer assertion (AC-06, R-02, R-03)
2. Live `search()` returns `Ok(_)` on graph-enabled layer — Layer 3 of above (SR-05, ADR-003)
3. `test_from_profile_returns_ok_on_cycle_error` — degraded mode (AC-05, R-04)
4. All existing `layer_tests.rs` and `eval/profile/tests.rs` tests pass unchanged (AC-08)

---

## Integration Harness Plan

**Conclusion:** `pytest -m smoke` is the mandatory minimum gate. No additional infra-001
suites are relevant. crt-045 modifies only `eval/profile/layer.rs` (eval path, CLI-only)
— no MCP tools, no protocol, no store schema, no confidence scoring. The critical
behavioral assertions require direct in-process access to `pub(crate)` accessors and raw
SQL insertion — not accessible through the MCP JSON-RPC protocol.

**No new infra-001 integration tests needed.**

---

## Key ADR Enforcement in Test Plans

| ADR | Enforcement Point |
|-----|-----------------|
| ADR-002 (degraded mode) | `test_from_profile_returns_ok_on_cycle_error` — asserts `Ok(layer)` + `use_fallback==true` on cycle |
| ADR-003 (three-layer test) | `test_from_profile_typed_graph_rebuilt_after_construction` — all three layers mandatory |
| ADR-004 (`pub(crate)` accessor) | Code review gate; no runtime test needed |
| ADR-005 (TOML `distribution_change=false`) | `test_ppr_expander_enabled_profile_parses_cleanly` unit test |

---

## Open Questions

None. All OQs from SCOPE.md are resolved. One implementation contingency noted:

**IR-04:** If `find_terminal_active` is not `pub(crate)` in `typed_graph.rs`, Layer 2
of the three-layer assertion must fall back to `graph_penalty()` as a proxy (acceptable
alternative documented in `layer_tests.md`). Do not add a visibility change to
`typed_graph.rs` without a scope variance flag.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #4096, #4099, #4100,
  #4101, #4102 (all crt-045 ADRs), #4097 (Arc clone pattern), #747 (test-support feature
  flag), #238 (testing conventions). ADRs directly incorporated into all test plan files.
- Stored: entry #4103 "Three-layer integration test for eval service layer graph wiring
  (wired-but-unused guard)" via `uni-store-pattern` — captures the three-layer assertion
  requirement and fixture constraints as a reusable testing pattern for future eval layer
  features.
