# Risk Coverage Report: bugfix-476

## Bug Summary

**GH #476**: `co_access_promotion_tick` promoted quarantined-endpoint pairs into
`GRAPH_EDGES`. The batch candidate SELECT had no JOIN against `entries` to check
endpoint status. Additionally, the scalar subquery for `max_count` normalization
was also missing the filter, causing inflated weights when quarantined pairs had
higher counts than any active pair.

**Fix**: Both the outer SELECT and the scalar subquery JOIN against `entries`
twice (once for each endpoint) with `status != ?3` (Quarantined = 3). A
`Status::Quarantined` bind was added as the third bind parameter.

**Files changed**:
- `crates/unimatrix-server/src/services/co_access_promotion_tick.rs` — SQL fix
- `crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs` — 3 new regression tests
- `crates/unimatrix-store/src/analytics.rs` — clarifying comment (no logic change)

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Quarantined entry promoted via co_access tick | `test_quarantine_one_endpoint_no_edges_promoted` | PASS | Full |
| R-02 | Both endpoints quarantined still promotes | `test_quarantine_both_endpoints_no_edges_promoted` | PASS | Full |
| R-03 | max_count inflated by quarantined high-count pair produces wrong weight | `test_quarantine_mixed_batch_only_active_pairs_promoted_with_correct_weight` | PASS | Full |
| R-04 | Regression on existing active-entry promotion behavior | Full workspace test suite (4264 unit tests) | PASS | Full |
| R-05 | Integration regression — lifecycle flows, quarantine + search | `test_lifecycle.py` suite (41 pass, 2 xfail expected) | PASS | Full |
| R-06 | Integration regression — smoke suite | All 22 smoke tests | PASS | Full |

---

## Test Results

### Unit Tests (Bug-Specific Regression Tests)

All three tests in
`crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs`:

| Test | Result |
|------|--------|
| `test_quarantine_one_endpoint_no_edges_promoted` | PASS |
| `test_quarantine_both_endpoints_no_edges_promoted` | PASS |
| `test_quarantine_mixed_batch_only_active_pairs_promoted_with_correct_weight` | PASS |

Command: `cargo test --package unimatrix-server --lib -- test_quarantine`

### Full Workspace Unit Tests

- Total: 4264 passed
- Failed: 0
- Ignored: 28 (pre-existing)

Command: `cargo test --workspace`

### Clippy

**Changed packages** (`unimatrix-server`, `unimatrix-store`, `--no-deps`):
- No errors in `co_access_promotion_tick.rs` or `analytics.rs` (the changed files).
- Pre-existing warnings/errors exist in `unimatrix-observe` and `unimatrix-engine`
  (58 errors confirmed on `d10dbd0`, the commit before this fix). These are
  unrelated to this bugfix and were not introduced by it.

### Integration Tests

#### Smoke Suite (`-m smoke`)
- Total: 22 passed
- Failed: 0

All 22 smoke tests passed in 191s.

#### Lifecycle Suite (`test_lifecycle.py`)
Most relevant to this fix area (quarantine, co_access, correction chains, status
lifecycle).

- Passed: 41
- xfailed: 2 (pre-existing: auto-quarantine tick timing, dead-knowledge tick timing)
- xpassed: 1 (`test_search_multihop_injects_terminal_active` — XPASS is
  pre-existing, first observed in bugfix-434 retro; GH#406 may have been
  incidentally fixed. Not caused by this PR. Marker cleanup tracked separately.)
- Failed: 0

---

## Gaps

None. All identified risks have test coverage and all tests pass.

The analytics drain write-time quarantine guard (filtering quarantined endpoints
at `co_access` insert time) is intentionally deferred to a follow-up (GH #477).
The tick-side JOIN is the authoritative gate; write-time filtering is a defense-
in-depth enhancement.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01: Quarantined endpoint pairs not promoted to graph_edges | PASS | `test_quarantine_one_endpoint_no_edges_promoted` passes; SQL JOIN confirmed in source |
| AC-02: Both-quarantined pairs not promoted | PASS | `test_quarantine_both_endpoints_no_edges_promoted` passes |
| AC-03: max_count subquery also excludes quarantined pairs (weight correctness) | PASS | `test_quarantine_mixed_batch_only_active_pairs_promoted_with_correct_weight` passes; weight=1.0 not 0.5 |
| AC-04: Active-only pairs continue to promote normally | PASS | Existing unit tests (4264 pass) + lifecycle integration suite (41 pass) |
| AC-05: No integration regression | PASS | Smoke (22/22), lifecycle (41/41 non-xfail) |

---

## Pre-existing Issues Observed (Not Fixed)

1. **Clippy**: 58 pre-existing errors in `unimatrix-observe` and `unimatrix-engine`.
   These existed before this fix (confirmed by checking `d10dbd0`). Not caused by
   or related to this PR.

2. **XPASS `test_search_multihop_injects_terminal_active`**: Pre-existing since
   bugfix-434. GH#406 marker should be removed in a cleanup PR.

3. **`col018_topic_signal_from_file_path` flakiness**: Passes in isolation and
   passed in the full workspace run today. Pre-existing embedding model init
   contention issue (Unimatrix entry #3714). No action in this PR.
