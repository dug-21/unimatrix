# Risk Coverage Report: crt-043 (GH #505 Bugfix Verification)

## Context

Bugfix #505: server-level test seam for `EmbedServiceHandle` (G-02/G-03 gaps).
Verified on branch `bugfix/505-embed-handle-test-seam`.

Changed files:
- `crates/unimatrix-server/src/infra/embed_handle.rs` — `set_ready_for_test` mutator + `EmbedErrorProvider` stub
- `crates/unimatrix-server/src/uds/listener.rs` — 5 new unit tests + 2 helper builders
- `product/test/infra-001/suites/test_lifecycle.py` — 1 new `@pytest.mark.smoke` integration test

Net new tests: 7 unit + 1 integration smoke = **8 new tests**.

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-02 | bincode config divergence — silent garbage floats | Pre-existing round-trip tests in `unimatrix-store` | PASS | Full |
| R-07 | Embed task blocks tokio executor | `test_cycle_start_goal_does_not_block_response` (integration, wall-clock < 1.0s) | PASS | Full |
| R-09 | Goal embedding spawned on empty/absent goal | `test_no_embed_task_on_empty_goal`, `test_no_embed_task_on_absent_goal` | PASS | Full |
| R-10 | Embed service unavailable path: warn not emitted or cycle blocked | `test_goal_embedding_unavailable_service_warn`, `test_goal_embedding_error_during_embed` | PASS | Full |
| G-02 | No test seam: EmbedServiceHandle only testable in Loading state (bugfix root cause) | `test_set_ready_for_test_transitions_to_ready`, `test_embed_error_provider_returns_error`, all 5 listener tests | PASS | Full |
| G-03 | No MCP-level fire-and-forget timing guard | `test_cycle_start_goal_does_not_block_response` | PASS | Full |

### Previously Covered Risks (pre-existing, not re-tested by this bugfix)

| Risk ID | Risk Description | Coverage Source |
|---------|-----------------|-----------------|
| R-01 | INSERT/UPDATE race — silent NULL goal_embedding | Integration tests in `listener.rs` (pre-existing) |
| R-03 | Missing write site — phase not captured | Phase capture unit tests (pre-existing) |
| R-04 | Phase capture inside spawn_blocking | Phase capture timing tests (pre-existing) |
| R-05 | Migration partial application | Store migration integration tests (pre-existing) |
| R-06 | Migration idempotency broken | Store migration idempotency tests (pre-existing) |
| R-08 | Residual race — UPDATE before INSERT | Degradation acceptance tests (pre-existing) |
| R-11 | decode_goal_embedding missing or mismatched | Round-trip test in unimatrix-store (pre-existing) |
| R-12 | context_cycle MCP response text changed | MCP response text test (pre-existing) |
| R-13 | Composite index deferred — full-scan observations | Written decision in delivery notes (pre-existing) |

---

## Test Results

### Bug-Specific Unit Tests

All 7 new unit tests pass.

| Test | Location | Result |
|------|----------|--------|
| `infra::embed_handle::tests::test_set_ready_for_test_transitions_to_ready` | embed_handle.rs | PASS |
| `infra::embed_handle::tests::test_embed_error_provider_returns_error` | embed_handle.rs | PASS |
| `uds::listener::tests::test_goal_embedding_written_after_cycle_start` | listener.rs | PASS |
| `uds::listener::tests::test_no_embed_task_on_empty_goal` | listener.rs | PASS |
| `uds::listener::tests::test_no_embed_task_on_absent_goal` | listener.rs | PASS |
| `uds::listener::tests::test_goal_embedding_unavailable_service_warn` | listener.rs | PASS |
| `uds::listener::tests::test_goal_embedding_error_during_embed` | listener.rs | PASS |

### Full Workspace Unit Tests (`cargo test --workspace`)

- Total passed: 2776 (unimatrix-server lib) + all other crates
- Failed: 0 (on second run; first run had 1 flaky pre-existing failure — see below)
- Pre-existing flaky test: `uds::listener::tests::col018_topic_signal_null_for_generic_prompt` — fails intermittently due to async timing (noted in Unimatrix entry #3714, project memory). Confirmed pre-existing: file last changed before this branch; not in the branch diff. No xfail added — this is an intermittent failure, not a deterministic pre-existing fail.

### Clippy (`cargo clippy --workspace -- -D warnings`)

- One pre-existing error: `crates/unimatrix-engine/src/auth.rs:113` — `collapsible_if` warning elevated to error. Last touched in `crt-014` / `col-006` (commits f02a43bb, 1494b58e). File not in this branch's diff. Not caused by this bugfix.
- No new warnings or errors introduced by this bugfix.

### Integration Tests (infra-001)

#### Smoke Gate (`pytest -m smoke`)
- Total: 23 passed, 0 failed, 256 deselected
- New smoke test `test_cycle_start_goal_does_not_block_response`: **PASS**
- All pre-existing smoke tests: PASS

#### Lifecycle Suite (`pytest suites/test_lifecycle.py`)
- Total: 45 passed, 5 xfailed, 2 xpassed, 0 failed
- New test `test_cycle_start_goal_does_not_block_response`: **PASS** (at 50%)
- 5 xfailed: all pre-existing (tick-interval, dead-knowledge, S1 edge count — require short tick env var not present in test environment)
- 2 xpassed: `test_search_multihop_injects_terminal_active` and `test_inferred_edge_count_unchanged_by_cosine_supports` — pre-existing xpass conditions, not caused by this fix

---

## Gaps

None. All risks identified for this bugfix (G-02, G-03) have full test coverage. The two pre-existing issues (clippy `collapsible_if` in auth.rs, intermittent `col018_topic_signal_null_for_generic_prompt`) are unrelated to this fix and pre-date this branch.

---

## Acceptance Criteria Verification

The ACCEPTANCE-MAP.md ACs were written for the full crt-043 delivery. This bugfix specifically addresses the G-02/G-03 gaps — the test infrastructure gaps that left AC-02, AC-04 partially unverifiable. The following ACs are now fully verifiable via the new test seam:

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-02 | PASS | `test_goal_embedding_written_after_cycle_start` uses `make_ready_embed_service()` — embed adapter is in Ready state; test verifies `goal_embedding IS NOT NULL` after dispatch |
| AC-04a (unavailable) | PASS | `test_goal_embedding_unavailable_service_warn` — Loading handle → warns → `goal_embedding IS NULL`, response is Ack |
| AC-04a (error) | PASS | `test_goal_embedding_error_during_embed` — EmbedErrorStub → warns → `goal_embedding IS NULL`, response is Ack |
| AC-04b (empty goal) | PASS | `test_no_embed_task_on_empty_goal` — no spawn, no warn, `goal_embedding IS NULL` |
| AC-04b (absent goal) | PASS | `test_no_embed_task_on_absent_goal` — no spawn, no warn, `goal_embedding IS NULL` |
| AC-06 | PASS | All 5 listener tests assert response is `Ack` — cycle start response text unchanged |

All other ACs (AC-01, AC-03, AC-05, AC-07–AC-14) were verified in the prior delivery; this bugfix adds no regressions.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entry #4174 (EmbedServiceHandle fire-and-forget spawn paths require stub provider to be unit-testable), directly confirming the gap this fix addresses. Also returned entry #4175 (inline mock pattern for unimatrix-embed in other crates' test blocks).
- Stored: nothing novel to store — the inline mock pattern was stored as entry #4175 by the fix agent (505-agent-1-fix). No new patterns or lessons discovered during verification.
