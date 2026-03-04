# vnc-008 Test Plan Overview

## Test Strategy

This is a pure restructuring feature. The primary risk is regression — ensuring all behavior is preserved after moving code. The test strategy prioritizes:

1. **Compilation verification** at each migration step (R-01)
2. **Behavioral equivalence** via existing test suite (R-02, R-03)
3. **New capability tests** for SessionWrite (R-05, R-08)
4. **Output equivalence tests** for format_status_change (R-04)

## Risk Mapping

| Risk ID | Test Type | Component Test Plan |
|---------|-----------|-------------------|
| R-01 | Compilation gate | All components — `cargo check` after each step |
| R-02 | Unit + integration | tool-context — build_context/require_cap tests |
| R-03 | Unit + snapshot | status-service — compute_report equivalence |
| R-04 | Unit (18 cases) | response-split — format_status_change vs originals |
| R-05 | Unit (serde) | session-write — Capability round-trip |
| R-06 | Compilation + count | infra-migration, mcp-migration, uds-migration — test count comparison |
| R-07 | Grep verification | All — import direction checks |
| R-08 | Unit + integration | session-write — UDS capability enforcement |
| R-09 | Compilation | response-split — visibility verification |
| R-10 | Compilation | infra-migration — re-export correctness |
| R-11 | Code review | status-service — table constant verification |

## Integration Harness Plan

### Existing Suites That Apply

1. **Unit tests in moved modules** — All `#[cfg(test)] mod tests` blocks move with their modules. Expected: same count, same results.

2. **Integration tests** (`tests/integration_tests.rs` or similar) — Any `use unimatrix_server::response::*` or `use unimatrix_server::registry::*` imports must be updated to `use unimatrix_server::mcp::response::*` and `use unimatrix_server::infra::registry::*` respectively. Or maintained via re-exports.

### New Integration Tests Needed

1. **ToolContext equivalence** — Verify that all 12 MCP handlers produce identical output before and after ToolContext introduction. Covered by existing handler integration tests passing unchanged.

2. **StatusService equivalence** — Snapshot test with known data comparing StatusService::compute_report() output to inline implementation output. Since the inline code is being replaced, the test verifies by checking the same data flow.

3. **UDS capability enforcement** — New integration test: construct a HookRequest for an operation that would require Write or Admin capability, send it through UDS dispatch, verify rejection.

4. **SessionWrite capability boundary** — New unit tests verifying:
   - UDS_CAPABILITIES contains exactly {Read, Search, SessionWrite}
   - SessionWrite permits session operations
   - SessionWrite does NOT permit knowledge writes or admin ops

5. **format_status_change equivalence** — 18 unit test cases (3 variants x 3 formats x 2 reason states) comparing generic output to original function output.

### Test Infrastructure

No new test infrastructure needed. Existing fixtures and helpers are sufficient. Tests use the same patterns as vnc-006/007.

## Baseline

Pre-vnc-008 test count: 1,664 passed, 18 ignored.
Post-vnc-008 target: >= 1,664 passed (plus new tests for SessionWrite, format_status_change, ToolContext).

## Verification Order

1. After infra-migration: `cargo test --workspace` (same count)
2. After mcp-migration + response-split: `cargo test --workspace` (same count)
3. After uds-migration: `cargo test --workspace` (same count)
4. After tool-context + status-service + session-write: `cargo test --workspace` (count increases from new tests)
5. After cleanup: `cargo test --workspace` (final count)
