# Risk Coverage Report: bugfix-519

## Context

GH #519: `topic_signal = NULL` for all observations when session is evicted before
`context_cycle(start)` arrives. Root cause: `set_feature_force` silently no-ops when
the session is absent from the registry, leaving every subsequent observation with
no feature attribution.

Verified on branch `bugfix/519-topic-signal-evicted-session`.

Changed files:
- `crates/unimatrix-server/src/uds/listener.rs` — `handle_cycle_event` pre-registers
  absent sessions before calling `set_feature_force` on `CycleLifecycle::Start`
- `crates/unimatrix-server/src/infra/session.rs` — (supporting changes)

New regression test:
- `uds::listener::tests::cycle_start_on_evicted_session_re_registers_and_attributes_observations`

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `set_feature_force` silently no-ops for absent session — cycle attribution lost | `cycle_start_on_evicted_session_re_registers_and_attributes_observations` | PASS | Full |
| R-02 | Session not re-registered after eviction via `drain_and_signal_session` | `cycle_start_on_evicted_session_re_registers_and_attributes_observations` (Step 2+4 assertions) | PASS | Full |
| R-03 | `feature` not set on re-registered session — subsequent observations still get NULL topic_signal | `cycle_start_on_evicted_session_re_registers_and_attributes_observations` (Step 5+8 assertions) | PASS | Full |
| R-04 | `current_phase` not populated from `next_phase` field on cycle_start payload | `cycle_start_on_evicted_session_re_registers_and_attributes_observations` (Step 6 assertion) | PASS | Full |
| R-05 | Regression: normal (non-evicted) cycle_start path broken by fix | Full workspace test suite — 2734 unit tests pass | PASS | Full |
| R-06 | Integration regression in lifecycle/cycle-review MCP paths | smoke gate (22 tests), lifecycle targeted suite (6 tests) | PASS | Full |

---

## Test Results

### Bug-Specific Regression Test

| Test | Result |
|------|--------|
| `uds::listener::tests::cycle_start_on_evicted_session_re_registers_and_attributes_observations` | PASS |

Test validates the full regression scenario end-to-end:
1. Register session, evict it via `drain_and_signal_session`
2. Confirm session is absent from registry
3. Dispatch `cycle_start` for evicted session
4. Assert session is re-registered
5. Assert `feature = "col-999"` on re-registered session
6. Assert `current_phase = "discovery"` from cycle_start payload
7. Dispatch follow-up `PreToolUse` observation with no explicit `topic_signal`
8. Assert observation in DB has `topic_signal = "col-999"` (attribution works end-to-end)

### Unit Tests (cargo test --workspace)

- Total: 2734 passed
- Failed: 0
- Ignored: 0

All test result lines showed `ok`. No failures anywhere in the workspace.

### Clippy (cargo clippy --workspace -- -D warnings)

No errors in the changed files (`uds/listener.rs`, `infra/session.rs`).

Pre-existing clippy errors exist in `unimatrix-observe/src/detection/session.rs` and
`unimatrix-observe/src/synthesis.rs` (collapsible_if, manual_pattern_char_comparison,
needless_let). These are unrelated to this bugfix — none touch the changed files and
were present before this branch. No new warnings introduced.

### Integration Tests (infra-001)

**Smoke gate (mandatory):**
- Command: `python -m pytest suites/ -v -m smoke --timeout=60`
- Total: 22 passed, 0 failed
- Result: PASS

**Targeted lifecycle tests (cycle/session/persistence paths):**
- `test_store_search_find_flow` — PASS
- `test_correction_chain_integrity` — PASS
- `test_isolation_no_state_leakage` — PASS
- `test_concurrent_search_stability` — PASS
- `test_data_persistence_across_restart` — PASS
- `test_phase_tag_store_cycle_review_flow` — PASS

Total integration tests run: 28 (22 smoke + 6 targeted lifecycle)
All passed. No xfail markers required.

Note: The session registry and UDS listener dispatch path are not exercised by the
MCP JSON-RPC integration harness (which uses stdio transport). The regression is fully
covered by the targeted unit test which exercises the UDS dispatch path directly.

---

## Gaps

None. All risks from the bug report have full test coverage. The regression test
validates the complete causal chain from session eviction through to per-observation
`topic_signal` attribution in the database.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01: cycle_start on evicted session re-registers the session | PASS | Step 4 assertion in regression test: `registry.get_state("sess-evicted")` is Some after dispatch |
| AC-02: feature_cycle set correctly on re-registered session | PASS | Step 5 assertion: `state.feature == Some("col-999")` |
| AC-03: current_phase set from next_phase in cycle_start payload | PASS | Step 6 assertion: `state.current_phase == Some("discovery")` |
| AC-04: Subsequent observations attributed to the correct feature (topic_signal not NULL) | PASS | Step 8 DB query: `topic_signal = "col-999"` for PreToolUse observation |
| AC-05: No regression in normal (non-evicted) cycle_start path | PASS | 2734 workspace unit tests pass; smoke gate passes |
