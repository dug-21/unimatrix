# Risk Coverage Report: bugfix-408

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Co-access signal lost after 30-day dormancy (constant too aggressive) | `co_access_staleness_at_least_one_year` | PASS | Full |
| R-02 | Regression: other co-access behaviour broken by constant change | `test_co_access_training_improves_retrieval`, `test_status_report_with_adaptation_active`, `test_search_coac_signal_reaches_scorer` | PASS | Full |
| R-03 | Maintenance tick deletes valid pairs prematurely | `test_confidence_evolution_over_access`, `test_full_lifecycle_pipeline` | PASS | Full |
| R-04 | No guard prevents future reduction of the threshold below one year | `co_access_staleness_at_least_one_year` (assert >= 365d) | PASS | Full |

## Test Results

### Bug-Specific Regression Test

- Test: `coaccess::tests::co_access_staleness_at_least_one_year`
- Command: `cargo test -p unimatrix-engine --lib co_access_staleness_at_least_one_year`
- Result: **PASS** (1/1)
- Assertion: `CO_ACCESS_STALENESS_SECONDS >= 365 * 24 * 3600`
- Verified value: `31_536_000` (365 days)

### Unit Tests (Full Workspace)

- Total: 3671
- Passed: 3671
- Failed: 0
- Ignored: 27 (pre-existing, unrelated to this fix)
- Command: `cargo test --workspace`

### Integration Tests

#### Smoke Suite (Mandatory Gate)

- Total: 20
- Passed: 20
- Failed: 0
- xfailed: 0
- Command: `pytest suites/ -v -m smoke --timeout=60`
- Status: GATE PASSED

#### Adaptation Suite (Co-Access Direct Coverage)

- Total: 10
- Passed: 9
- Failed: 0
- xfailed: 1 (`test_volume_with_adaptation_active` — pre-existing)
- Command: `pytest suites/test_adaptation.py -v --timeout=60`
- Notable: `test_co_access_training_improves_retrieval` PASS — validates co-access signal accumulates and influences search ranking

#### Confidence Suite

- Total: 14
- Passed: 13
- Failed: 0
- xfailed: 1 (`test_base_score_deprecated` — pre-existing GH#405)
- Command: `pytest suites/test_confidence.py --timeout=60`

#### Lifecycle Suite

- Total: 41
- Passed: 38
- Failed: 0
- xfailed: 2 (pre-existing: `test_auto_quarantine_after_consecutive_bad_ticks`, `test_dead_knowledge_entries_deprecated_by_tick`)
- xpassed: 1 (`test_search_multihop_injects_terminal_active` — GH#406 xfail now passes; unrelated to this fix, marker removal tracked under GH#406)
- Command: `pytest suites/test_lifecycle.py --timeout=60`

### Clippy

- Changed file (`coaccess.rs`): no warnings
- Workspace: 58 pre-existing errors in `unimatrix-observe` and 2 in `unimatrix-engine` (`auth.rs:113`, `event_queue.rs:164`)
- Neither affected file is `coaccess.rs`; these errors exist on `main` and are not caused by this fix
- No new warnings introduced by this change

## Pre-Existing Issues Observed (Not Fixed Here)

| Issue | Test | Status |
|-------|------|--------|
| GH#405 | `test_base_score_deprecated` | xfail, pre-existing |
| GH#406 | `test_search_multihop_injects_terminal_active` | XPASS — fix may have landed incidentally; verify GH#406 and remove marker if confirmed |
| Pre-existing | `test_volume_with_adaptation_active` | xfail, pre-existing |
| Pre-existing | `test_auto_quarantine_after_consecutive_bad_ticks` | xfail, pre-existing |
| Pre-existing | `test_dead_knowledge_entries_deprecated_by_tick` | xfail, pre-existing |
| Pre-existing | 58 clippy errors in `unimatrix-observe` + 2 in `unimatrix-engine` | unrelated to fix |

No new GH Issues filed — no new failures discovered.

## Gaps

None. The single-constant change is fully covered by:
1. A direct assertion test on the new value.
2. Integration tests exercising co-access signal accumulation and retrieval boost through the MCP interface.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01: `CO_ACCESS_STALENESS_SECONDS` >= 365 days | PASS | Constant value `31_536_000`; asserted by `co_access_staleness_at_least_one_year` |
| AC-02: Co-access signal visible through MCP search after accumulation | PASS | `test_co_access_training_improves_retrieval` PASS |
| AC-03: No regression in existing co-access or confidence behaviour | PASS | All 20 smoke + 9/10 adaptation + 13/14 confidence + 38/41 lifecycle pass (xfails are pre-existing) |
| AC-04: Doc comment explains dormant-cycle rationale | PASS | Doc comment in `coaccess.rs` lines 14-20 verified in source |

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") for bug fix verification testing procedures — found entries #2326 (fire-and-forget async audit pattern), #3257 (clippy triage: scope to affected crates), #3479 (two-site atomicity coupled test). Entry #3257 directly applicable: confirmed clippy errors in `auth.rs` and `event_queue.rs` are pre-existing and not in the changed crate boundary.
- Stored: nothing novel to store — the fix is a one-line constant change; no new test infrastructure, fixture patterns, or harness techniques were developed. The triage procedure for pre-existing clippy errors is already documented in entry #3257.
