# Gate 3c Report: vnc-016

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-05-18
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 10 risks covered; RISK-COVERAGE-REPORT.md maps every risk to passing test(s) |
| Test coverage completeness | PASS | All risk-to-scenario mappings from Phase 2 exercised; integration smoke + tools + lifecycle + security all pass |
| Specification compliance | PASS | All 13 AC from SPECIFICATION.md verified; all FR-01 through FR-07 implemented |
| Architecture compliance | PASS | Component boundaries, SQL fix, write_capable field, and MCP wiring match ARCHITECTURE.md exactly |
| Knowledge stewardship compliance | WARN | Tester agent report present with Queried/Stored entries; R-05 GH issue "required at PR time" not yet confirmed created |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 10 risks from RISK-TEST-STRATEGY.md to passing tests:

| Risk | Mitigated By |
|------|-------------|
| R-01 (feature_entries absent) | `test_dependency_on_deprecated_e2e` step 4 uses enrolled Restricted+Write agent with `feature_cycle=cycle_id` |
| R-02 (memoized result) | Both integration tests contain `force=True` literal; unique `cycle_id` per test via `uuid.uuid4().hex[:8]` |
| R-03 (Rust positive path tautological) | `test_query_stale_prerequisite_edges_for_cycle_returns_pair` has all three sub-assertions: `result.is_ok()`, `pairs.len() == 1`, `pairs[0] == (id_a as u64, id_b as u64)` — confirmed by direct inspection at read.rs:3385-3403 |
| R-04 (negative companion absent) | `test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry` present; seeds graph_edges without feature_entries row; asserts empty result |
| R-05 (unwrap_or_else re-conceals regression) | Store-layer unit test bypasses tools.rs error swallowing; integration test provides end-to-end coverage; both layers verified present |
| R-06 (Restricted agent skips feature_entries) | Enrolled `test_agent_id` with `trust_level="restricted"` and `capabilities=["write", "read"]` used at step 4; `write_capable: true` set in handler (confirmed tools.rs:836) |
| R-07 (cycle_id mismatch) | Single `cycle_id` binding at top of each test function; same variable passed to all three setup calls |
| R-08 (negative assertion too broad) | `not any(h["rule_name"] == "dependency_on_deprecated" ...)` assertion — not `hotspots == []` |
| R-09 (feature_cycle forwarded as null) | Guard `if feature_cycle is not None: args["feature_cycle"] = feature_cycle` present at client.py:415 |
| R-10 (existing call sites broken) | Full tools suite: 155 passed, 3 xfailed (pre-existing) |

**Inconsistency noted and resolved**: RISK-TEST-STRATEGY.md R-06 "Test Scenarios" item 1 states "Both integration tests must call `context_store` with `agent_id='human'`" — this is stale text that predates the usage gate fix design in Phase 2a. The SPECIFICATION (FR-05.2 step 4, C-01b) and ARCHITECTURE.md (Production Bug Fix section) both correctly require a Restricted+Write agent. The implementation follows the spec. The RISK-COVERAGE-REPORT correctly documents this resolution. Not a failure.

---

### 2. Test Coverage Completeness

**Status**: PASS

**Evidence**: All risk-to-scenario mappings from Phase 2 are exercised:

- **Smoke suite**: 23/23 passed (0 failed, 0 xfailed) — confirms server starts and basic tools respond
- **Tools suite** (`test_tools.py`): 155 passed, 0 failed, 3 xfailed (all pre-existing: GH#405, GH#305, GH#575)
- **Lifecycle suite** (`test_lifecycle.py`): 52 passed, 0 failed, 5 xfailed (pre-existing tick/ONNX timing), 2 xpassed (positive regression, pre-existing xfail markers now passing — cleanup recommended)
- **Security suite** (`test_security.py`): 20/20 passed

New vnc-016 integration tests:
- `test_dependency_on_deprecated_e2e`: PASS
- `test_dependency_on_deprecated_no_finding_without_stale_edge`: PASS

New vnc-016 Rust unit tests:
- `read::tests::test_query_stale_prerequisite_edges_for_cycle_returns_pair`: PASS
- `read::tests::test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry`: PASS
- `services::usage::usage_tests::test_write_capable_false_yields_no_feature_recording`: PASS
- `services::usage::usage_tests::test_write_capable_true_yields_feature_recording`: PASS

`cargo test --workspace`: all test result lines show 0 failed, 0 FAILED output lines observed.

All xfail markers in `test_tools.py` reference GH issue numbers (GH#405, GH#305, GH#575). No new xfail markers were introduced by vnc-016.

The 2 XPASS cases in lifecycle suite are pre-existing xfail markers where the underlying issue was incidentally fixed by prior work. These are positive regressions. The coverage report correctly calls for cleanup in a follow-up. No GH issue number is cited in the report for this cleanup, but it is not a blocking concern.

---

### 3. Specification Compliance

**Status**: PASS

**Evidence**: All acceptance criteria verified:

| AC-ID | Verification | Result |
|-------|-------------|--------|
| AC-01 | `test_dependency_on_deprecated_e2e` at test_tools.py:3628, pytest exits 0 | PASS |
| AC-02 | 9-step scenario in order: enroll → store A (Restricted+Write agent) → store B → edge → deprecate → seed → review | PASS |
| AC-03 | `any(rn == "dependency_on_deprecated" for rn in rule_names)` at test_tools.py:3748 | PASS |
| AC-04 | `fe.feature_id` at read.rs:1618; `fe.feature_cycle` absent from file (grep confirms) | PASS |
| AC-05 | `feature_cycle: str \| None = None` parameter at client.py:395; guard at lines 415-416 | PASS |
| AC-06 | `cargo test --workspace`: 0 failed; full pytest suite: 0 failed | PASS |
| AC-07 | `force=True` present in both test function bodies (test_tools.py:3721, :3806) | PASS |
| AC-08 | `test_dependency_on_deprecated_no_finding_without_stale_edge` at test_tools.py:3758, pytest exits 0 | PASS |
| AC-09 | `test_query_stale_prerequisite_edges_for_cycle_returns_pair` at read.rs:3329; all three sub-assertions present | PASS |
| AC-10 | `write_capable: bool` at usage.rs:79; no `#[serde(default)]`; no `Default` derivation; cargo build exits 0 | PASS |
| AC-11 | `record_mcp_usage` gate (lines 212-218) and `record_hook_injection` gate (lines 273-279) both use `ctx.write_capable`; `TrustLevel` not referenced in either gate block | PASS |
| AC-12 | `context_enroll` with `trust_level="restricted"` and `capabilities=["write","read"]` precedes `context_store` for entry A; `context_store` uses `agent_id=test_agent_id` | PASS |
| AC-13 | `test_write_capable_false_yields_no_feature_recording` and `test_write_capable_true_yields_feature_recording` at usage.rs:1343 and :1377 | PASS |

All functional requirements implemented:
- FR-01 (SQL fix): `fe.feature_id` confirmed at read.rs:1618
- FR-02 (Rust unit test): positive + negative companion both present and passing
- FR-03 (client.py extension): `feature_cycle` parameter added with correct guard; `uds_client.py` unmodified
- FR-04 (usage gate fix): `write_capable: bool` field added; both gate blocks updated; all construction sites explicitly set field; `trust_level` retained
- FR-05 (positive integration test): 9-step scenario implemented correctly
- FR-06 (negative integration test): independent cycle_id; correct absence assertion
- FR-07 (gate logic unit tests): both branches covered

Non-functional requirements:
- NFR-01: All existing tests pass without regression
- NFR-02: `cargo test -p unimatrix-store` exits 0
- NFR-03: No new MCP tools or schema changes
- NFR-04: No new Python dependencies
- NFR-05: Each test function is self-contained (unique cycle_id, unique agent_id)
- NFR-06: `cargo build --workspace` exits 0 (clippy state: 19 warnings in unimatrix-server but pre-existing, no -D warnings run at gate time)
- NFR-07: `write_capable` has no default; exhaustive construction enforced
- NFR-08: `trust_level` field retained on `UsageContext`

---

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:

- **SQL fix** (Component 1): `query_stale_prerequisite_edges_for_cycle` at read.rs:1607 uses `fe.feature_id = ?1` at line 1618. Function signature and return type `Result<Vec<(u64, u64)>>` unchanged. Single-token change as specified.
- **Rust unit test** (Component 2): Located in `read.rs mod tests` at lines 3329–3464 per ADR-001. Uses `open_test_store` + `write_pool` seeding pattern. No new test infrastructure. Both positive and negative variants present.
- **Client extension** (Component 3): `feature_cycle` parameter added after `edges` as keyword-only arg. Guard uses `if feature_cycle is not None:` pattern. `uds_client.py` unmodified per architecture requirement.
- **Integration tests** (Component 4): Located in vnc-015 section of `test_tools.py` after line 3048. Unique cycle ID and agent ID per invocation. `force=True` mandatory in both tests.
- **Component interactions**: The data flow path documented in ARCHITECTURE.md (test_tools.py → client.py:context_store → UsageService → feature_entries → query_stale_prerequisite_edges_for_cycle → DependencyOnDeprecatedRule) is fully exercised by the positive-path test.
- **Production bug fix** (usage.rs): `write_capable: bool` field added to `UsageContext` at line 79. Both gate blocks replaced per ADR-002. `write_capable: true` set only at tools.rs:836 (inside `require_cap(Write)` branch). All other construction sites set `write_capable: false`.
- **Error swallowing** (SR-01/R-05): `unwrap_or_else` at tools.rs:2174 is deliberately preserved per architecture scope decision. Rust unit test at store layer is the regression guard. Architecture constraint is documented and acknowledged.

Minor naming inconsistency: ARCHITECTURE.md §Integration Test Structure refers to `_resolve_db_path` but the implementation uses `_compute_db_path` (the existing helper in the test suite). This is a documentation artifact, not an implementation error — the correct helper is used.

---

### 5. Knowledge Stewardship Compliance

**Status**: WARN

**Evidence**:
- RISK-COVERAGE-REPORT.md was produced as the testing artifact. It does not contain a separate `## Knowledge Stewardship` section, but this is the coverage report, not an agent execution report.
- The SPECIFICATION.md contains a `## Knowledge Stewardship` section with `Queried:` entries documenting `context_briefing` usage and the lessons returned (entries #4444, #4411, ADR-007 #83). This is present in the design phase document.
- The RISK-COVERAGE-REPORT.md notes at line 151-153: "A GitHub issue for hardening this to `ERROR`-level logging is required at PR time per the implementation brief." No GH issue number is cited in the report, indicating the issue may not yet be filed. This is the sole open stewardship action item.

**WARN basis**: The R-05 follow-up GH issue is described as "required at PR time" but no issue number appears in the coverage report. This is not a blocking gate failure but must be resolved before merge.

---

## Rework Required

None. All gate checks PASS (one WARN accepted).

---

## Pre-Merge Action Item (Not a Gate Block)

**R-05 GH Issue**: The coverage report states "A GitHub issue for hardening this to `ERROR`-level logging is required at PR time." This issue must be filed before the PR is merged. The change is out of scope for vnc-016; the issue tracks future hardening of `unwrap_or_else` at tools.rs:2174 to use `tracing::error!` instead of `tracing::warn!`.

**Lifecycle XPASS cleanup**: Two lifecycle tests marked `@pytest.mark.xfail` are now passing (xpassed). These pre-existing markers should be removed in a follow-up. Not a vnc-016 action item.

---

## Knowledge Stewardship

- Stored: nothing novel to store — this feature's gate patterns (R-06 stale risk strategy vs. spec supersession, Rust unit test as sole regression guard for error-swallowing handlers) are feature-specific gate results that live in this report, not recurring cross-feature patterns warranting a lesson-learned entry.
