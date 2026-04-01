# Risk Coverage Report: bugfix-473

## Bug Summary

Phase 5 of `run_nli_detection_tick` incorrectly used the Supports cap (`MAX_EDGES_PER_TICK`)
to gate Informs candidates, causing the Informs budget to be stolen by Supports fill and
making Informs edges unreachable when Supports filled first. The fix introduces an independent
budget constant `MAX_INFORMS_PER_TICK = 25` applied exclusively to the Informs pool.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Informs budget stolen by Supports fill — Informs edges never written when Supports pool is full | `test_phase5_informs_always_gets_dedicated_budget` | PASS | Full |
| R-02 | Small Informs pool incorrectly capped when pool < MAX_INFORMS_PER_TICK | `test_phase5_informs_small_pool_all_kept` | PASS | Full |
| R-03 | Empty Informs pool handled safely (no panic, no out-of-bounds) | `test_phase5_informs_empty_pool_stays_empty` | PASS | Full |
| R-04 | Shuffle + truncate produces valid deduplicated candidate set | `test_phase5_informs_shuffle_no_duplicates_valid_ids` | PASS | Full |
| R-05 | Log accounting invariant: dropped + kept == total | `test_phase5_informs_log_accounting_consistent` | PASS | Full |
| R-06 | Regression: contradiction/NLI detection tick overall correctness | `test_contradiction.py` (13 tests) | PASS | Full |
| R-07 | Regression: multi-step lifecycle flows not disrupted by fix | `test_lifecycle.py` (41 passing) | PASS | Full |
| R-08 | Regression: smoke baseline (system-wide capability) | `pytest -m smoke` (22 tests) | PASS | Full |

## Test Results

### Unit Tests — Bug-Specific

| Test | Module | Result |
|------|--------|--------|
| `test_phase5_informs_always_gets_dedicated_budget` | `services::nli_detection_tick::tests` | PASS |
| `test_phase5_informs_small_pool_all_kept` | `services::nli_detection_tick::tests` | PASS |
| `test_phase5_informs_empty_pool_stays_empty` | `services::nli_detection_tick::tests` | PASS |
| `test_phase5_informs_shuffle_no_duplicates_valid_ids` | `services::nli_detection_tick::tests` | PASS |
| `test_phase5_informs_log_accounting_consistent` | `services::nli_detection_tick::tests` | PASS |

### Unit Tests — Full Workspace

- Total: 4261
- Passed: 4261
- Failed: 0
- Ignored: 28

All 4261 tests pass across all crates.

### Clippy

- Command: `cargo clippy --workspace -- -D warnings`
- Changed file (`nli_detection_tick.rs`): **0 errors, 0 warnings** introduced by this fix
- Pre-existing errors in `unimatrix-observe` (54 errors in `source.rs`, `metrics.rs`,
  `extraction/shadow.rs`, `attribution.rs`): unrelated to this fix, present on `main` before
  the fix was applied. Not caused by bugfix-473.

### Integration Tests

#### Smoke Suite (`-m smoke`)

- Total: 22
- Passed: 22
- Failed: 0
- Duration: 191s

#### Contradiction Suite (`test_contradiction.py`)

- Total: 13
- Passed: 13
- Failed: 0
- Duration: 108s

#### Lifecycle Suite (`test_lifecycle.py`)

- Total: 44 collected
- Passed: 41
- xfailed: 2 (pre-existing, GH#406 and GH#291 — known, unrelated to this fix)
- xpassed: 1 (pre-existing xfail that is now passing — not caused by this fix, no action needed)
- Failed: 0

## Gaps

None. All risks identified for this bug fix have direct unit test coverage via the 5
new test functions. Regression coverage is provided by contradiction (13), lifecycle (41),
and smoke (22) integration tests.

## Failure Triage

No integration test failures were observed. The 2 xfail results in the lifecycle suite are
pre-existing with documented GH Issues (GH#406, GH#291) and are unrelated to this fix.
The 1 xpassed result indicates a pre-existing issue may be self-healing — no action taken
per triage protocol (do not fix unrelated issues in this PR).

## Clippy Pre-existing Issues

The 58 clippy errors from `cargo clippy --workspace -- -D warnings` are all in `unimatrix-observe`,
specifically `src/source.rs`, `src/metrics.rs`, `src/extraction/shadow.rs`, and
`src/attribution.rs`. These exist identically on `main` before this fix and are not caused by
bugfix-473. They require a separate bug fix or cleanup ticket.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01: Informs budget is independent of Supports fill | PASS | `test_phase5_informs_always_gets_dedicated_budget` — Supports fills MAX_INFORMS_PER_TICK slots, Informs still receives its full MAX_INFORMS_PER_TICK budget |
| AC-02: Pool smaller than budget keeps all candidates | PASS | `test_phase5_informs_small_pool_all_kept` — pool of MAX/2 entries, all kept |
| AC-03: Empty pool handled safely | PASS | `test_phase5_informs_empty_pool_stays_empty` — no panic, empty result |
| AC-04: Shuffle produces valid deduplicated output | PASS | `test_phase5_informs_shuffle_no_duplicates_valid_ids` — no duplicates, IDs within valid range |
| AC-05: Log accounting invariant maintained | PASS | `test_phase5_informs_log_accounting_consistent` — dropped + kept == total for all cases |
| AC-06: No regression in contradiction/NLI detection | PASS | 13/13 contradiction integration tests pass |
| AC-07: No regression in lifecycle flows | PASS | 41/41 lifecycle integration tests pass (xfails pre-existing) |
| AC-08: Smoke gate passes | PASS | 22/22 smoke integration tests pass |
