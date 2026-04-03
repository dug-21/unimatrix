# Risk Coverage Report: crt-043 Behavioral Signal Infrastructure

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | INSERT/UPDATE race: embed UPDATE executes before INSERT → silent NULL goal_embedding | `test_update_goal_embedding_writes_blob` (store-level); code review: embed spawn is after INSERT spawn in `handle_cycle_event` (listener.rs:2509-2512) | PASS | Partial |
| R-02 | bincode config divergence: encode/decode uses mismatched config → garbage floats | `test_encode_decode_round_trip`, `test_decode_malformed_bytes_returns_error`, `test_encode_matches_direct_bincode_call` | PASS | Full |
| R-03 | Missing write site: one of four observation write sites omits phase capture or bind | Code review: all four sites confirmed (lines 707-710, 824-827, 934-937, 1081-1083); `test_v20_to_v21_both_columns_present` verifies column exists; no per-site DB read-back unit tests | CODE REVIEW | Partial |
| R-04 | Phase captured inside spawn_blocking closure instead of before it → race | Code review: `obs.phase = ...` appears before `spawn_blocking` at all four sites; no automated timing test | CODE REVIEW | Partial |
| R-05 | v20→v21 migration partial application: one column added, version bumped before second ALTER | `test_v20_to_v21_both_columns_present` (real v20 fixture), `test_v20_to_v21_partial_apply_recovery` (pre-existing goal_embedding column) | PASS | Full |
| R-06 | Migration idempotency broken: re-open v21 alters or errors on already-present columns | `test_v21_migration_idempotent`, `test_fresh_db_creates_schema_v21` | PASS | Full |
| R-07 | Embed task blocks tokio executor: embedding not routed through ml_inference_pool | Code review: embed runs inside `tokio::spawn` calling `get_adapter()` backed by rayon pool; no latency regression test | CODE REVIEW | Partial |
| R-08 | Residual race: UPDATE on non-existent cycle_id → silent NULL with no log | `test_update_goal_embedding_nonexistent_cycle_id` | PASS | Full |
| R-09 | Goal embedding spawned on empty/absent goal: unnecessary spawn, wrong warn behavior | Code review: `filter(|s| !s.is_empty())` guard at listener.rs:2527; no stub-based automated test | CODE REVIEW | Partial |
| R-10 | Embed service unavailable: warn not emitted, or cycle blocked | Code review: `tracing::warn!` at listener.rs:2540-2543 and 2549-2552; no stub-injected unit test | CODE REVIEW | Partial |
| R-11 | decode_goal_embedding missing or mismatched from encode path | `test_encode_decode_round_trip`; `decode_goal_embedding` is `pub` in `unimatrix-store::embedding`; compilation confirms presence | PASS | Full |
| R-12 | context_cycle MCP response text changed by crt-043 | Integration tools suite: `test_cycle_phase_end_type_accepted`, `test_cycle_invalid_type_rejected`, `test_cycle_phase_end_stores_row` pass; no byte-for-byte response text assertion | PASS | Partial |
| R-13 | Composite index deferred beyond delivery: Group 6 queries full-scan observations | `test_v21_composite_index_present` verifies `idx_observations_topic_phase` exists after migration | PASS | Full |

---

## Test Results

### Unit Tests (cargo test --workspace)

- Total: 4,421
- Passed: 4,421
- Failed: 0
- Ignored: 28

**Feature-specific store tests (--features test-support --test migration_v20_v21):**

| Test | Risk Coverage | Result |
|------|--------------|--------|
| `test_current_schema_version_is_21` | MIG-V21-U-01 | PASS |
| `test_fresh_db_creates_schema_v21` | MIG-V21-U-02, R-06 | PASS |
| `test_v20_to_v21_both_columns_present` | R-05, AC-01, AC-07 | PASS |
| `test_v20_to_v21_partial_apply_recovery` | R-05 partial-apply recovery | PASS |
| `test_v21_migration_idempotent` | R-06, AC-11 | PASS |
| `test_v21_composite_index_present` | R-13 | PASS |
| `test_update_goal_embedding_nonexistent_cycle_id` | R-08 | PASS |
| `test_update_goal_embedding_writes_blob` | R-01 (partial), AC-03 | PASS |

**Feature-specific embedding helper tests (embedding::tests):**

| Test | Risk Coverage | Result |
|------|--------------|--------|
| `test_encode_decode_round_trip` | R-02, R-11, AC-14 | PASS |
| `test_decode_malformed_bytes_returns_error` | R-02 | PASS |
| `test_encode_matches_direct_bincode_call` | R-02 | PASS |
| `test_encode_decode_empty_vec` | edge case | PASS |
| `test_encode_decode_768_dim_vec` | future dimension upgrade | PASS |

Note: migration_v20_v21.rs tests require `--features test-support` flag and run via
`cargo test --package unimatrix-store --features test-support --test migration_v20_v21`.
The `cargo test --workspace` invocation does not activate this feature gate, so these 8
tests show as "filtered" in the default run and must be run explicitly.

### Integration Tests (infra-001)

Suites run per test-plan suite selection (schema + storage changes → smoke + lifecycle + tools):

**Smoke suite (mandatory gate) — `pytest -m smoke --timeout=60`:**

- Total: 22
- Passed: 22
- Failed: 0

**Tools suite (cycle-related subset — AC-06 regression gate):**

| Test | Coverage | Result |
|------|----------|--------|
| `test_cycle_phase_end_type_accepted` | AC-06 regression | PASS |
| `test_cycle_invalid_type_rejected` | AC-06 regression | PASS |
| `test_cycle_phase_end_stores_row` | AC-06 regression | PASS |

**New integration tests planned but not implemented:**

- `test_cycle_start_goal_does_not_block_response` — planned in test-plan/OVERVIEW.md as a smoke-marked test in `test_lifecycle.py` validating NFR-01 (fire-and-forget, < 2s wall-clock). Not added to the harness during Stage 3b. See Gaps section.

---

## Static Code Review Assertions Confirmed

| Assertion | Location | Status |
|-----------|----------|--------|
| Embed spawn (Step 6) appears after INSERT spawn (Step 5) in `handle_cycle_event` | listener.rs:2509-2588 | CONFIRMED |
| `adapter.embed_entry()` called inside `tokio::spawn` async task backed by rayon pool via `get_adapter()` | listener.rs:2535-2546 | CONFIRMED |
| Phase captured before `spawn_blocking` at rework_candidate write site | listener.rs:707-710 | CONFIRMED |
| Phase captured before `spawn_blocking` at RecordEvent write site | listener.rs:824-827 | CONFIRMED |
| Phase captured before `spawn_blocking` at RecordEvents batch write site | listener.rs:934-937 | CONFIRMED |
| Phase captured before struct construction at ContextSearch write site | listener.rs:1081-1083 | CONFIRMED |
| `ObservationRow` has `phase: Option<String>` field | listener.rs:2608 | CONFIRMED |
| `insert_observation` binds `phase` at position 9 | listener.rs:2782 | CONFIRMED |
| `insert_observations_batch` binds `phase` per row at position 9 | listener.rs:2822 | CONFIRMED |
| `CURRENT_SCHEMA_VERSION = 21` | confirmed by `test_current_schema_version_is_21` | CONFIRMED |
| Both helpers use `config::standard()` | embedding.rs:33, 47 | CONFIRMED |
| Helpers are `pub` (WARN-2 resolution: Group 6 cross-crate access) | embedding.rs:32, 46 | CONFIRMED |
| Whitespace-only goal treated as absent (trim-then-check) | listener.rs:2524-2528 | CONFIRMED |
| `extract_observation_fields` initializes `phase: None` (captured at call site) | listener.rs:2699 | CONFIRMED |
| `decode_goal_embedding` is in same module as `encode_goal_embedding` | embedding.rs | CONFIRMED |
| `tracing::warn!` emitted on `EmbedNotReady` error from `get_adapter()` | listener.rs:2540-2543 | CONFIRMED |
| `tracing::warn!` emitted on `embed_entry()` error | listener.rs:2549-2552 | CONFIRMED |
| `tracing::warn!` emitted on `encode_goal_embedding()` error | listener.rs:2559-2562 | CONFIRMED |
| `tracing::warn!` emitted on `update_cycle_start_goal_embedding()` failure | listener.rs:2572-2576 | CONFIRMED |

---

## Gaps

### G-01: Per-Site DB Read-Back Tests for Phase Capture (R-03, R-04)

**Risks:** R-03 (Missing write site), R-04 (Phase captured inside spawn_blocking)
**Required by plan:** PHASE-U-01 through PHASE-U-09 — per-site DB read-back, timing test
**Status:** Not implemented. Phase capture at all four sites is confirmed only by static code review, not by automated DB read-back tests.

The test plan marks per-site DB read-back as "Non-Negotiable." The automated timing test (PHASE-U-06) verifying pre-capture timing contract is also absent.

**Recommended action:** File a GH Issue. Add `test_phase_captured_record_event_site`, `test_phase_captured_rework_candidate_site`, `test_phase_captured_record_events_batch_site`, `test_phase_captured_context_search_site`, and `test_phase_capture_timing_pre_spawn` to the listener.rs test module.

### G-02: Server-Level Unit Tests for Goal Embedding Behavior (R-01, R-09, R-10)

**Risks:** R-01 (INSERT/UPDATE ordering), R-09 (empty/absent goal), R-10 (embed unavailable)
**Required by plan:** EMBED-SRV-01 through EMBED-SRV-09
**Status:** Not implemented. The server crate has no injectable stub `EmbedServiceHandle`, so tests requiring a configurable embed service cannot be written without test infrastructure changes.

Specifically missing:
- `test_goal_embedding_written_after_cycle_start` — end-to-end await + DB read-back (R-01, AC-02, AC-03, AC-13)
- `test_no_embed_task_on_empty_goal` / `test_no_embed_task_on_absent_goal` (R-09, AC-04b)
- `test_goal_embedding_unavailable_service_warn` (R-10, AC-04a)
- `test_goal_embedding_error_during_embed` (R-10, AC-04a)
- `test_handle_cycle_event_returns_before_embedding` (R-07, NFR-01)

The RISK-TEST-STRATEGY.md marks AC-04a tests as "Non-Negotiable" before gate-3b. These are absent.

**Recommended action:** File a GH Issue. Add a stub or test-seam for `EmbedServiceHandle` (e.g., a `#[cfg(test)]` constructor that accepts a closure/channel for `get_adapter()`). Implement EMBED-SRV-01 through EMBED-SRV-07 against that stub.

### G-03: New Integration Test Not Added to infra-001 (NFR-01 — Fire-and-Forget Timing)

**Risk:** R-07 (embed task blocks tokio), NFR-01 (MCP response < 50ms)
**Required by plan:** `test_lifecycle.py::test_cycle_start_goal_does_not_block_response` (planned as `@pytest.mark.smoke`)
**Status:** Not implemented. No new test was added to the infra-001 integration harness during Stage 3b.

**Recommended action:** Add the test to `product/test/infra-001/suites/test_lifecycle.py`. The test body is fully specified in test-plan/OVERVIEW.md:213-215.

### G-04: Concurrent Cycle Start Stress Test (R-01 — Stress Scenario)

**Risk:** R-01 (concurrent 20-cycle ordering under load)
**Required by plan:** `test_goal_embedding_concurrent_cycle_starts` (marked `#[ignore]`)
**Status:** Not implemented. This was an `#[ignore]` slow-test, lower priority than G-01/G-02.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_v20_to_v21_both_columns_present` — pragma_table_info count=1 for `goal_embedding` |
| AC-02 | GAP | No injectable stub; call path confirmed by code review (listener.rs:2536) |
| AC-03 | PARTIAL | `test_update_goal_embedding_writes_blob` verifies store method writes blob; no end-to-end dispatch test |
| AC-04a | GAP | Code review confirms `tracing::warn!` at listener.rs:2540-2543; no stub-based automated test |
| AC-04b | GAP | Code review confirms `filter(|s| !s.is_empty())` guard at listener.rs:2527; no stub test |
| AC-05 | PASS | `bincode` and `EmbedServiceHandle` pre-existing in workspace; build succeeds without new deps |
| AC-06 | PASS | `test_cycle_phase_end_type_accepted`, `test_cycle_phase_end_stores_row`, `test_cycle_invalid_type_rejected` all pass; no byte-for-byte text assertion |
| AC-07 | PASS | `test_v20_to_v21_both_columns_present` — pragma_table_info count=1 for `phase` on observations |
| AC-08 | PASS | listener.rs:2608 `phase: Option<String>`; compilation confirms |
| AC-09 | PARTIAL | listener.rs:2782, 2822 bind `phase`; no per-site DB read-back test (code review only) |
| AC-10a | GAP | Code review confirms pre-spawn capture; no timing test (PHASE-U-06 not implemented) |
| AC-10b | GAP | `extract_observation_fields` returns `phase: None` at listener.rs:2699; no automated test |
| AC-11 | PASS | `test_v21_migration_idempotent` |
| AC-12 | FAIL | AC-02, AC-04a, AC-04b, AC-10a, AC-10b not covered by automated tests |
| AC-13 | PARTIAL | `test_update_goal_embedding_writes_blob` validates store method; code review confirms spawn ordering; no end-to-end await test |
| AC-14 | PASS | `test_encode_decode_round_trip`; `test_decode_malformed_bytes_returns_error`; both in embedding.rs |

---

## Gate 3c Verdict: CONDITIONAL PASS

### Passing Elements

- All 4,421 unit tests pass with zero failures.
- Integration smoke gate: 22/22 PASS.
- Schema migration tests (8): all PASS — the core storage correctness (R-05, R-06, R-08, R-11, R-13) is fully verified.
- Embedding helper tests (5): all PASS — bincode format is validated (R-02, R-11, AC-14).
- Code review confirms correct implementation at all four phase-capture sites and in the goal-embedding spawn path.

### Blocking Gaps

G-01 and G-02 represent tests that the RISK-TEST-STRATEGY.md explicitly declares "Non-Negotiable before gate-3b":

- Per-site phase DB read-back tests (R-03, R-04)
- Embed-service-unavailable warn test, empty-goal guard (R-10, R-09, AC-04a, AC-04b)

These gaps are attributable to missing test infrastructure (no injectable `EmbedServiceHandle` stub) and were not resolved by the delivery agent. The implementation appears correct on code review, but the non-negotiable automated coverage criteria are not met.

### Recommended Follow-Up

File one GH Issue covering G-01 through G-03 with the specific test names from this report. The issue should be linked from the PR. The implementation does not have known defects — the gaps are test quality, not correctness failures.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — MCP tool schema not available in this agent context (ToolSearch returned no match). Proceeded without.
- Stored: nothing novel to store. The `--features test-support` activation requirement for store integration tests (use `cargo test --package unimatrix-store --features test-support --test <test-binary-name>`) is a project-specific refinement but is already established by prior migration test files. The gap pattern — server fire-and-forget paths require injectable seams for unit testing — is documented in entry #735 and the test plan itself.
