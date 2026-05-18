# Risk Coverage Report: vnc-016

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Integration test passes vacuously — `feature_entries` row absent for entry A | `test_dependency_on_deprecated_e2e` (positive path, step 4 uses `test_agent_id` with `feature_cycle=cycle_id`); `test_write_capable_true_yields_feature_recording` | PASS | Full |
| R-02 | Integration test passes vacuously — memoized result returned (`force=False`) | Both integration tests contain `force=True` literal; unique `cycle_id` per test via `uuid.uuid4().hex[:8]` | PASS (code inspection) | Full |
| R-03 | Rust positive-path test assertion is structurally always-true | `test_query_stale_prerequisite_edges_for_cycle_returns_pair` — all three sub-assertions present: `result.is_ok()`, `pairs.len() == 1`, `pairs[0] == (A_id, B_id)` | PASS | Full |
| R-04 | Rust negative-path companion absent — broken JOIN scoping undetectable | `test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry` — present, asserts empty when no `feature_entries` row exists | PASS | Full |
| R-05 | Future SQL regression re-concealed by `unwrap_or_else` in `tools.rs:2169` | `test_query_stale_prerequisite_edges_for_cycle_returns_pair` (store-layer, bypasses `unwrap_or_else`); `test_dependency_on_deprecated_e2e` (integration) | PASS | Full — both layers present |
| R-06 | `context_store` with unenrolled/Restricted agent — old trust gate silently skips `feature_entries` | `test_dependency_on_deprecated_e2e` step 4 uses enrolled Restricted+Write agent (`test_agent_id`), NOT `"human"`; `test_write_capable_true_yields_feature_recording` confirms gate path | PASS | Full |
| R-07 | Observation cycle_id mismatch — empty-cycle early-exit triggered | Both tests bind `cycle_id` once at top of function; same variable passed to `_seed_observation_sql` and `context_cycle_review` | PASS | Full |
| R-08 | Negative-path assertion is too broad — misses always-fires regression | `test_dependency_on_deprecated_no_finding_without_stale_edge` uses `not any(h["rule_name"] == "dependency_on_deprecated" ...)`, not `hotspots == []` | PASS | Full |
| R-09 | `feature_cycle` forwarded as explicit JSON `null` | Code inspection: `if feature_cycle is not None: args["feature_cycle"] = feature_cycle` guard present in `client.py:415` | PASS (code inspection) | Full |
| R-10 | Existing call sites broken by `client.py` signature change | Full tools suite: 155 passed, 3 xfailed (pre-existing); all existing `context_store` callers unaffected | PASS | Full |

---

## Test Results

### Unit Tests

- Total (all crates, `cargo test --workspace`): 4,900
- Passed: 4,900
- Failed: 0
- Ignored: 28

Specific vnc-016 new tests:

| Test | Crate | Result |
|------|-------|--------|
| `read::tests::test_query_stale_prerequisite_edges_for_cycle_returns_pair` | `unimatrix-store` | PASS |
| `read::tests::test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry` | `unimatrix-store` | PASS |
| `services::usage::usage_tests::test_write_capable_false_yields_no_feature_recording` | `unimatrix-server` | PASS |
| `services::usage::usage_tests::test_write_capable_true_yields_feature_recording` | `unimatrix-server` | PASS |

### Integration Tests

| Suite | Tests | Passed | Failed | XFailed | XPassed |
|-------|-------|--------|--------|---------|---------|
| Smoke (`-m smoke`) | 23 | 23 | 0 | 0 | 0 |
| Tools (`test_tools.py`) | 158 | 155 | 0 | 3 | 0 |
| Lifecycle (`test_lifecycle.py`) | 59 | 52 | 0 | 5 | 2 |
| Security (`test_security.py`) | 20 | 20 | 0 | 0 | 0 |

**Total integration tests run**: 260
**Total passed**: 250
**Total failed**: 0
**Pre-existing xfails**: 8 (all pre-existing, none caused by vnc-016)
**XPassed (pre-existing xfail now passing)**: 2 (pre-existing, not vnc-016 related)

New vnc-016 integration tests:

| Test | Suite | Result |
|------|-------|--------|
| `test_dependency_on_deprecated_e2e` | `test_tools.py` | PASS |
| `test_dependency_on_deprecated_no_finding_without_stale_edge` | `test_tools.py` | PASS |

### XFailed Tests

All xfail markers are pre-existing (not introduced by vnc-016):

**Tools suite (3 xfailed)**: Pre-existing markers — not caused by vnc-016 changes to `read.rs`, `usage.rs`, `tools.rs`, `client.py`, or `test_tools.py`.

**Lifecycle suite (5 xfailed)**:
- `test_dead_knowledge_entries_deprecated_by_tick` — pre-existing tick-interval timing issue
- `test_context_status_supports_edge_count_increases_after_tick` — pre-existing tick-interval timing issue
- `test_s1_edges_visible_in_status_after_tick` — pre-existing tick-interval timing issue
- `test_inferred_edge_count_unchanged_by_s1_s2_s8` — pre-existing timing issue
- (1 additional pre-existing)

**Lifecycle suite (2 xpassed)**:
- `test_inferred_edge_count_unchanged_by_cosine_supports` — pre-existing xfail marker that now passes; no GH Issue filed here as this is a positive regression (test passing unexpectedly due to prior fix). This should be noted to the human for cleanup.

No new xfail markers were introduced by vnc-016.

---

## Code Inspection Results

### AC-04: SQL Fix (`read.rs:1618`)
- `grep -n 'fe\.feature_id' read.rs` → line 1618: `AND fe.feature_id = ?1`
- `grep -n 'fe\.feature_cycle' read.rs` → no results
- Status: PASS

### AC-05: Harness Client Extension (`client.py`)
- `feature_cycle: str | None = None` parameter present at line 395
- Guard `if feature_cycle is not None: args["feature_cycle"] = feature_cycle` at lines 415-416
- All existing `context_store` call sites unmodified (confirmed by full tools suite pass)
- Status: PASS

### AC-07: `force=True` in both integration tests
- `test_dependency_on_deprecated_e2e`: `force=True` present on `context_cycle_review` call (step 9)
- `test_dependency_on_deprecated_no_finding_without_stale_edge`: `force=True` present on `context_cycle_review` call
- Status: PASS

### AC-10: `UsageContext.write_capable` field declaration
- `write_capable: bool` declared at `usage.rs:79`
- No `#[serde(default)]` present adjacent to the field
- No `Default` derivation on `UsageContext`
- `cargo build --workspace` exits 0 — all construction sites explicitly set the field
- Status: PASS

### AC-11: Gate blocks use `ctx.write_capable` only
- `record_mcp_usage` gate block (lines ~207-218): `if ctx.write_capable { ... }` — `TrustLevel` not referenced
- `record_hook_injection` gate block (lines ~272-283): `if ctx.write_capable { ... }` — `TrustLevel` not referenced
- Status: PASS

### AC-12: Construction site audit (`tools.rs`)
- One `write_capable: true` occurrence at line 836 (`context_store` handler, inside `require_cap(Write)` branch)
- All other construction sites: `write_capable: false`
- Status: PASS

---

## Gaps

None. All 10 risks from RISK-TEST-STRATEGY.md have test coverage. No risks are uncovered.

**R-09 (Low)** is verified by code inspection rather than an executable test, as specified in the risk strategy (serde missing-key vs null contract — architecture analysis is sufficient; no additional test scenario needed).

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `pytest -k test_dependency_on_deprecated_e2e` exits 0; test exists in `test_tools.py:3628` |
| AC-02 | PASS | Code inspection: 9 steps in order (enroll → store A → store B → edge → deprecate → seed → review); `test_agent_id` used at step 4 |
| AC-03 | PASS | Code inspection: `any(h["rule_name"] == "dependency_on_deprecated" for h in hotspots)` present |
| AC-04 | PASS | `grep` confirms `fe.feature_id` at line 1618; `fe.feature_cycle` absent from `read.rs` |
| AC-05 | PASS | Code inspection: `feature_cycle: str \| None = None` in signature; guard present; full pytest suite passes (R-10) |
| AC-06 | PASS | `cargo test --workspace`: 4,900 passed, 0 failed; full pytest smoke + tools + lifecycle + security: 0 failed |
| AC-07 | PASS | Code inspection: `force=True` present in both test function bodies |
| AC-08 | PASS | `pytest -k test_dependency_on_deprecated_no_finding_without_stale_edge` exits 0 |
| AC-09 | PASS | `cargo test -p unimatrix-store --lib test_query_stale_prerequisite_edges_for_cycle`: 2 passed; all three sub-assertions present in test body (`is_ok()`, `len() == 1`, `[0] == (A_id, B_id)`) |
| AC-10 | PASS | `write_capable: bool` at `usage.rs:79`; no `#[serde(default)]`; `cargo build --workspace` exits 0 |
| AC-11 | PASS | Both gate blocks (`record_mcp_usage`, `record_hook_injection`) use `ctx.write_capable`; `TrustLevel` not referenced in either gate block |
| AC-12 | PASS | Code inspection: `context_enroll` with `trust_level="restricted"` and `capabilities=["write","read"]` precedes `context_store` for entry A; `context_store` for entry A uses `agent_id=test_agent_id` |
| AC-13 | PASS | `cargo test -p unimatrix-server --lib test_write_capable`: 2 passed (`test_write_capable_false_yields_no_feature_recording`, `test_write_capable_true_yields_feature_recording`) |

---

## Notes

### Pre-existing XPASS in lifecycle suite

Two lifecycle tests (`test_inferred_edge_count_unchanged_by_cosine_supports` and one additional) are marked `@pytest.mark.xfail` but now pass (`XPASS`). These are pre-existing xfail markers from prior features and are unrelated to vnc-016. They represent a positive regression — the underlying bugs were incidentally fixed by prior work. The xfail markers should be removed in a follow-up cleanup, but this is not a vnc-016 action item.

### `unwrap_or_else` in `tools.rs:2169` (R-05 follow-up)

Per ALIGNMENT-REPORT.md WARN and R-05, the `unwrap_or_else` at `tools.rs:2169` that silently swallows SQL errors remains unchanged by vnc-016. The Rust unit test at the store layer is the regression guard. A GitHub issue for hardening this to `ERROR`-level logging is required at PR time per the implementation brief.
