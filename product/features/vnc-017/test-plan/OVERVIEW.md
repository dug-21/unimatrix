# Test Plan Overview: vnc-017 — Auto-Redirect Incoming Edges on context_correct

## Overall Test Strategy

Three test levels apply to this feature:

**Unit tests (Rust)** — In-memory SQLite via `sqlx`. Cover `query_incoming_edges` directly on the store, the redirect loop logic in the `context_correct` handler (stub-based where `redirect_graph_edge` cannot be easily isolated), and all response text format variants. These cover the 11 Rust ACs: AC-03, AC-04, AC-05, AC-08, AC-09, AC-10, AC-11, AC-13, AC-14, AC-17, plus the R-01 compile-time structural test, R-02 Supersedes exclusion structural test, R-05 ceiling tests, R-06 mixed-status tests, R-10 Phase B collision test.

**Integration tests (Python infra-001)** — Exercise the full `context_correct` MCP call end-to-end through the binary. These are the only tests that verify the response text appears in the actual `CallToolResult` and that the `graph_edges` table state is correct after a real correction cycle. Required for AC-01, AC-02, AC-06, AC-07, AC-10, AC-12, AC-16.

**Regression pass** — Existing `context_edge(mode="redirect")` test suite run unchanged (AC-15, R-13). No modifications permitted unless NFR-09 was violated.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test Level | Component | Key Scenarios |
|---------|----------|-----------|-----------|---------------|
| R-01 | Critical | Unit (structural) | redirect_loop | Match arms handle `Ok(())` + `Err(EdgeRedirectError)` only; no `Ok(bool)` arm exists |
| R-02 | Critical | Unit (structural) | query_incoming_edges | Call on Supersedes-only target returns empty Vec — validates SQL-level exclusion |
| R-03 | High | Unit | query_incoming_edges | High-cardinality seed (1000 non-target rows, 3 target rows) — validates WHERE filter correctness |
| R-04 | High | Unit | redirect_loop | 10-edge call: assert `store.get` called exactly 10 times (one per edge) |
| R-05 | High | Unit | redirect_loop | 55-edge seed: assert exactly 50 redirects, `tracing::warn!` with `total_found=55`, response contains truncation text |
| R-06 | Critical | Unit + Integration | redirect_loop | Quarantined source: `skipped++`, no `failed++`; Deprecated source same; mixed Active+Quarantined fan-in |
| R-07 | High | Unit | query_incoming_edges | Supersedes-only incoming edges: `query_incoming_edges` returns empty, no response append |
| R-08 | High | Integration | redirect_loop | Full redirect then graph tick: no DependencyOnDeprecated event (AC-16) |
| R-09 | High | Unit | redirect_loop | UNIQUE-conflict (`Ok(())`) counts `redirected++`; assert `failed == 0`, no `tracing::warn!` |
| R-10 | High | Unit | redirect_loop | Phase B + loop double-write: exactly one `C → B` row post-call, `failed == 0` |
| R-11 | High | Integration | response_format | Response text substring match for exact format string in real `CallToolResult` |
| R-12 | Med | Unit | query_incoming_edges | SQL-level exclusion means Supersedes-only target returns empty and emits no info log |
| R-13 | Med | Regression | (existing tests) | All existing `context_edge(mode="redirect")` tests pass unchanged |
| R-14 | Low | Code review only | redirect_loop | TOCTOU race accepted; no test gate; comment required at call site |

---

## Cross-Component Test Dependencies

1. `redirect_loop` depends on `query_incoming_edges`: the redirect loop tests that exercise the full path (AC-14) use a real in-memory store, which means `query_incoming_edges` must work correctly for redirect loop unit tests to be meaningful. The `query_incoming_edges` component tests run first in the component ordering.

2. `response_format` depends on `redirect_loop`: the 4-variant format table is driven by the `RedirectSummary` accumulator produced by the loop. Unit tests for `response_format` use pre-constructed `RedirectSummary` values; the integration tests verify the format appears in the real MCP response produced by the full loop path.

3. AC-16 (DependencyOnDeprecated detection after redirect) depends on vnc-016 tick infrastructure being in place — integration test must trigger a graph state tick explicitly.

---

## Integration Harness Plan

### Applicable Suites

| Suite | Reason to Run |
|-------|--------------|
| `smoke` | Mandatory minimum gate — must pass before merge |
| `tools` | `context_correct` is tool logic; the new redirect behavior surfaces in tool responses |
| `lifecycle` | Multi-step flow: store → edge → correct → assert graph state; restart persistence unchanged |

Suites NOT required: `confidence`, `contradiction`, `security`, `volume`, `edge_cases`, `protocol`. This feature adds no new tool, no protocol change, no schema migration, no security boundary.

### Existing Suite Coverage of This Feature

**tools suite** (`test_tools.py`) — The existing `test_correct_*` tests cover the basic `context_correct` happy path and error cases. They do NOT currently assert `graph_edges` state post-correction or the redirect summary text. These gaps are filled by new tests.

**lifecycle suite** (`test_lifecycle.py`) — Existing correction chain tests (`test_correct_chain_*`) verify the Supersedes chain and status transitions. They do NOT currently seed incoming edges pointing at the deprecated entry. AC-01, AC-02, AC-06 integration scenarios are new.

### New Integration Tests Required

All new tests go into `suites/test_lifecycle.py` (multi-step correction flows with graph state assertions) except AC-12 which may go in `test_tools.py` (response text format for `context_correct`).

| AC / Risk | Suite | Test Function Name | Fixture | What It Asserts |
|-----------|-------|--------------------|---------|-----------------|
| AC-01, AC-02, AC-06 | `test_lifecycle.py` | `test_correct_auto_redirects_prerequisite_edges` | `server` | After `context_correct(A→B)` with C→A (Prerequisite): no rows with `target_id=A` non-Supersedes; rows with `target_id=B` contain C |
| AC-07 | `test_lifecycle.py` | `test_correct_auto_redirects_contradicts_edges` | `server` | Contradicts edge pair: both `C→B` and `B→C` exist post-redirect |
| AC-10 | `test_lifecycle.py` | `test_correct_leaves_supersedes_edges_unchanged` | `server` | Supersedes row `target_id=A` survives; no Supersedes row `target_id=B` inserted |
| AC-12, R-11 | `test_lifecycle.py` | `test_correct_response_text_contains_redirect_summary` | `server` | `CallToolResult` text contains `"Redirected 2 incoming edges (0 failed, see logs)"` as substring |
| AC-16, R-08 | `test_lifecycle.py` | `test_correct_redirected_edges_clear_dependency_detection` | `server` | After auto-redirect and graph tick, no DependencyOnDeprecated event for the redirected source |

### Integration Tests NOT Needed

- Response format variants for truncation and partial failure: these are unit-testable via stub injection (the MCP integration layer has no way to inject SQL errors deterministically). Unit tests at AC-13 and R-05 are sufficient.
- AC-03 (terminal-active = new_entry.id, not chain traversal): structural unit test + code review gate is sufficient; no integration test needed.
- R-10 (Phase B double-write idempotency): the `INSERT OR IGNORE` behavior is SQLite-level; a Rust unit test with in-memory DB is sufficient.

---

## Minimum Merge Gate

1. `cargo test --workspace` passes (all unit tests).
2. `pytest -m smoke` passes (integration smoke gate).
3. `pytest suites/test_tools.py suites/test_lifecycle.py -v --timeout=60` passes.
4. All AC-01 through AC-17 verified.
5. R-01 (compile-time structural), R-02 (Supersedes exclusion structural), R-06 (mixed-status Contradicts) scenarios pass.
6. AC-15 (existing `context_edge(mode="redirect")` tests unchanged and passing).
