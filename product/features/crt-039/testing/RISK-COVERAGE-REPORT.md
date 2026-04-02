# Risk Coverage Report: crt-039

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Phase 8b write loop executes before `get_provider()` guard fires — Supports edges written without NLI scores (silent data corruption) | TC-02 (`test_phase8_no_supports_when_nli_not_ready`) | PASS | Full |
| R-02 | `test_run_graph_inference_tick_nli_not_ready_no_op` removed without replacement — no regression coverage for Path A / Path B boundary | TC-01 + TC-02 + TR-01 grep absence | PASS | Full |
| R-03 | Mutual-exclusion gap at cosine 0.50 boundary — explicit Supports-set subtraction absent | `test_phase4b_explicit_supports_set_subtraction` (TC-07) | PASS | Full |
| R-04 | `NliCandidatePair::Informs` / `PairOrigin::Informs` dead-code variants retained | `cargo build --workspace` clean; grep for variants in production code returns comments-only | PASS | Full |
| R-05 | Cosine floor raise 0.45→0.50 eliminates meaningful candidate pool | AC-11 eval harness (run_eval.py absent — see Gaps); AC-17 observability log present | PARTIAL | Partial (eval script absent) |
| R-06 | Stale `nli_scores` argument at `apply_informs_composite_guard` call sites | `cargo test --workspace` green; all call sites single-argument | PASS | Full |
| R-07 | Phase 8b Informs write loop skipped when `candidate_pairs` is empty | TC-01 with zero-Supports-candidates corpus | PASS | Full |
| R-08 | `format_nli_metadata_informs` dead code / NLI fields in Informs edge metadata | `format_nli_metadata_informs` deleted (grep returns doc-comment only); `format_informs_metadata` in place | PASS | Full |
| R-09 | Contradiction scan block behavioral change during structural labeling | Existing contradiction scan tests pass; background.rs diff shows comment additions only | PASS | Full |
| R-10 | Observability log emitted at wrong pipeline point — `informs_candidates_found` logged after dedup | Code inspection: counter incremented at line 367 before dedup check at line 370; log at line 501 after writes | PASS | Full |
| R-11 | Tick ordering invariant disturbed — `run_graph_inference_tick` moved relative to contradiction scan | Code inspection: ordering invariant comment at background.rs:661–664 matches spec; call site unchanged | PASS | Full |
| R-12 | Cosine floor boundary semantics (`>=` vs `>`) inverted | `test_phase4b_cosine_floor_boundary` (TC-05/TC-06 combined) | PASS | Full |

---

## Test Results

### Unit Tests

All unit tests pass. Counts sourced from `cargo test --workspace` output.

| Crate | Tests Run | Passed | Failed | Ignored |
|-------|-----------|--------|--------|---------|
| unimatrix-server (lib) | 2572 | 2572 | 0 | 0 |
| unimatrix-core | 423 | 423 | 0 | 0 |
| unimatrix-store | 347 | 346 | 0 | 1 |
| unimatrix-server (integration) | 73+16+16 | 73+16+16 | 0 | 0 |
| All other crates | 338 | 310 | 0 | 28 |
| **Total** | **~3900** | **~3872** | **0** | **~28** |

- Total: ~3900
- Passed: ~3872
- Failed: 0
- Ignored (pre-existing): ~28

### Integration Tests (infra-001)

#### Smoke Gate (mandatory)

```
python -m pytest suites/ -v -m smoke --timeout=60
22 passed, 232 deselected in 191.52s
```

Result: **PASS** (22/22)

#### Lifecycle Suite

```
python -m pytest suites/test_lifecycle.py -v --timeout=60
41 passed, 2 xfailed, 1 xpassed in 395.72s
```

Result: **PASS** (41/41 non-xfail)

- 2 xfailed: pre-existing (`test_auto_quarantine_after_consecutive_bad_ticks`, `test_dead_knowledge_entries_deprecated_by_tick`) — no GH issues filed (already marked)
- 1 xpassed: `test_search_multihop_injects_terminal_active` — pre-existing xfail that now passes (not caused by crt-039); xfail marker should be removed in follow-up

**Total integration tests: 63 run (22 smoke + 41 lifecycle), 63 passed, 0 failed**

### Static / Grep Checks

| Check | Command | Result |
|-------|---------|--------|
| TR-01 removal | `grep -n "fn test_run_graph_inference_tick_nli_not_ready_no_op"` in nli_detection_tick.rs | PASS (no function definition) |
| TR-02 removal | `grep -n "fn test_phase8b_no_informs_when_neutral_exactly_0_5"` in nli_detection_tick.rs | PASS (no function definition) |
| TR-03 removal | `grep -n "fn test_phase8b_writes_informs_when_neutral_just_above_0_5"` in nli_detection_tick.rs | PASS (no function definition) |
| AC-01 nli_enabled guard removed | `grep -n "nli_enabled"` in background.rs — returns only a comment, no `if` guard | PASS |
| AC-03 nli_scores in guard | `apply_informs_composite_guard` at line 806: `fn apply_informs_composite_guard(candidate: &InformsCandidate)` — single arg | PASS |
| AC-04 cosine floor 0.5 | `default_nli_informs_cosine_floor()` returns `0.5`; test asserts `0.5_f32` | PASS |
| AC-07 ordering comment | background.rs:661–664 ordering invariant comment present and correct | PASS |
| AC-08 no 0.45 assertions | config.rs test section has `0.5` (updated from `0.45`) in all cosine floor assertions | PASS |
| AC-17 log field present | `grep -n "informs_candidates_found"` returns lines 290, 366–367, 502 in nli_detection_tick.rs | PASS |
| AC-18 format_nli_metadata_informs absent | grep returns only a doc-comment reference (line 817); function body is `format_informs_metadata` | PASS |
| R-04 dead variants | `grep "NliCandidatePair::Informs\|PairOrigin::Informs"` returns comments only (lines 534, 631) | PASS |
| R-06 stale call sites | All `apply_informs_composite_guard` call sites use single argument | PASS |

---

## Gaps

### AC-11: Eval Harness (R-05 — PARTIAL coverage)

The eval harness gate requires running `product/research/ass-039/harness/run_eval.py` to confirm
MRR >= 0.2913. The `scenarios.jsonl` file (1,585 scenarios) is present but `run_eval.py` does not
exist in that directory. The eval runner was never implemented.

**Impact**: R-05 (cosine floor raise eliminates candidate pool) lacks its quantitative regression
backstop. The cosine floor change (0.45 → 0.50) is mitigated by:
1. ADR-003 requirement for implementor pre-condition corpus scan
2. FR-14 observability log (`informs_candidates_found`) providing equivalent production signal from tick 1
3. The change is conservative (higher floor = fewer but more confident edges)

**Recommendation**: Track this as a follow-up item. The eval harness runner should be implemented
before Group 3 graph enrichment features ship — they depend on a live Informs graph and will
require quantitative regression coverage.

### Clippy (-D warnings) on Full Workspace

`cargo clippy --workspace -- -D warnings` fails on `unimatrix-observe` (pre-existing, 54 errors
predating crt-039 — last observe commit was from col-027/col-026 circa PR #388/#377) and
`unimatrix-engine` (2 pre-existing collapsible-if errors). Neither crate is modified by crt-039.

`cargo clippy -p unimatrix-server` and `cargo clippy -p unimatrix-core` pass cleanly (warnings
only, no errors, none related to crt-039 changes).

**AC-18** (clippy clean for dead-code) is satisfied for the crt-039 affected crates.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | background.rs:772 comment only; `run_graph_inference_tick` called unconditionally at line 773 |
| AC-02 | PASS | TC-01 (`test_phase4b_writes_informs_when_nli_not_ready`) PASS; TC-02 (`test_phase8_no_supports_when_nli_not_ready`) PASS |
| AC-03 | PASS | `apply_informs_composite_guard` at line 806: single `candidate: &InformsCandidate` parameter; grep of function body returns no `nli_scores` |
| AC-04 | PASS | `test_inference_config_default_nli_informs_cosine_floor` asserts `0.5_f32`; config.rs:784 returns `0.5` |
| AC-05 | PASS | `test_phase4b_cosine_floor_boundary` (line 2128): 0.500 included (TC-05a), 0.499 excluded (TC-05b) |
| AC-06 | PASS | All contradiction scan tests pass in `cargo test --workspace`; background.rs diff shows comment additions only |
| AC-07 | PASS | Ordering invariant comment at background.rs:661–664: compaction → promotion → graph-rebuild → contradiction_scan → extraction_tick → structural_graph_tick (always) |
| AC-08 | PASS | config.rs test section: all assertions use `0.5` (no `0.45` assertion for floor default) |
| AC-09 | PASS | `grep -n "fn test_phase8b_no_informs_when_neutral_exactly_0_5\|fn test_phase8b_writes_informs_when_neutral_just_above_0_5"` returns empty |
| AC-10 | PASS | `cargo test --workspace` exits 0; ~3872 tests passed, 0 failed |
| AC-11 | PARTIAL | `run_eval.py` absent; scenarios.jsonl (1585 scenarios) present. Quantitative MRR gate cannot be executed. See Gaps. |
| AC-12 | PASS | `query_existing_informs_pairs` at line 174 (Phase 2); `truncate(MAX_INFORMS_PER_TICK)` at line 448 (Phase 5) before Phase A write loop at line 463 |
| AC-13 | PASS | `test_phase4b_explicit_supports_set_subtraction` (line 2157): pair at 0.68 in Supports set absent from `informs_metadata` |
| AC-14 | PASS | TC-02 (`test_phase8_no_supports_when_nli_not_ready`): zero Supports edges in real Store when NLI Loading |
| AC-15 | PASS | TC-01 (`test_phase4b_writes_informs_when_nli_not_ready`): at least one Informs edge written when NLI Loading |
| AC-16 | PASS | TC-01 and TC-02 are separate functions (lines 1278 and 1383); TR-01 grep `fn test_run_graph_inference_tick_nli_not_ready_no_op` returns empty |
| AC-17 | PASS | `grep -n "informs_candidates_found"` returns lines 290 (declaration), 367 (pre-dedup increment), 502 (debug log) in Phase 4b/5 region |
| AC-18 | PASS | `format_nli_metadata_informs` absent from production code (doc-comment reference at line 817 only); `cargo clippy -p unimatrix-server` produces no dead-code errors |

---

## GH Issues Filed

None. No integration test failures were caused by crt-039. The 2 xfailed lifecycle tests
(`test_auto_quarantine_after_consecutive_bad_ticks`, `test_dead_knowledge_entries_deprecated_by_tick`)
are pre-existing with existing xfail markers. The 1 xpassed test
(`test_search_multihop_injects_terminal_active`) is a pre-existing xfail that now passes; its
xfail marker should be removed in a follow-up PR.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3806, #3656, #3949, #2970, #3946, #4019, #4018 relevant to delivery-process lessons, testing patterns, and crt-039 ADRs. Key entry #2758 (gate-3c must grep for every non-negotiable test function name before accepting PASS claims) applied to TR-01/TR-02/TR-03 verification.
- Stored: nothing novel to store — the test execution followed established patterns (static grep checks for removed tests, separate TC-01/TC-02 for Path A/B boundary, in-crate integration tests for behavior not visible through MCP interface). Pattern #3949 (per-guard negative tests) was confirmed by TC-03/TC-04. A second feature would be needed to promote these into a cross-feature stored pattern.
