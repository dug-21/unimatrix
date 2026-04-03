# Gate 3c Report: crt-041

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-04-02
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | 14/17 risks fully covered; 3 partially covered with documented rationale |
| Test coverage completeness | WARN | R-04 timing test absent; R-06 crash-simulation test absent; R-13 dedicated unit test absent — all gaps accepted with rationale |
| Specification compliance | PASS | All 32 ACs verified; AC-16 and AC-30 partial but documented; AC-32 deferred (manual gate) |
| Architecture compliance | PASS | All components match architecture; tick ordering correct; ADRs followed |
| Integration smoke gate | PASS | 22/22 smoke tests pass |
| Integration test xfail compliance | FAIL | Two crt-041 xfail tests lack GH Issue reference required by USAGE-PROTOCOL.md |
| Pre-existing XPASS (crt-040) | WARN | `test_inferred_edge_count_unchanged_by_cosine_supports` is XPASS; not caused by crt-041 but marker should be cleaned up |
| Knowledge stewardship | PASS | Queried and Stored entries present in RISK-COVERAGE-REPORT.md |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**:

The RISK-COVERAGE-REPORT.md maps all 17 risks to test results:

- **R-01** (dual-endpoint quarantine guard): Full coverage. Six tests across S1, S2, S8 covering both source_id and target_id positions. Integration test `test_quarantine_excludes_endpoint_from_graph_traversal` passes (8.31s).
- **R-02** (S2 SQL injection): Full coverage. `test_s2_sql_injection_single_quote`, `test_s2_sql_injection_double_dash` both pass. Code review confirms `push_bind` used at every construction point with SECURITY comment present.
- **R-03** (InferenceConfig dual-site divergence): Full coverage. `test_inference_config_s1_s2_s8_defaults_match_serde` — the delivery-blocking AC — passes. Three tests confirm all 5 fields match between `impl Default` and serde.
- **R-04** (S1 GROUP BY performance): Partial. No timing test present. Corpus at ~500 entries is below 1,200-entry threshold. Risk accepted at current scale; gap documented.
- **R-05** (S8 watermark stuck on malformed JSON): Full coverage. `test_s8_watermark_advances_past_malformed_json_row` passes; watermark advances to 3 past malformed row 2.
- **R-06** (S8 watermark ordering): Partial. `test_s8_idempotent` covers idempotency; explicit crash-simulation absent. Code review confirms Phase 6 (watermark update) is placed after Phase 5 (edge writes). Risk accepted.
- **R-07** (S1/S2/S8 edges tagged source='nli'): Full coverage. Six tests confirm correct source values. Named constants confirmed: `EDGE_SOURCE_S1="S1"`, `EDGE_SOURCE_S2="S2"`, `EDGE_SOURCE_S8="S8"`.
- **R-08** (crt-040 prerequisite absent): Full coverage. `write_graph_edge` confirmed at nli_detection.rs:78.
- **R-09** (orphaned edges): Closed. Verified by background.rs:513-515 compaction SQL.
- **R-10** (S8 cap semantics): Full coverage. `test_s8_pair_cap_not_row_cap`, `test_s8_partial_row_watermark_semantics` both pass.
- **R-11** (S2 false-positive substring): Full coverage. False-positive suppression and true-positive both tested.
- **R-12** (S8 wrong operation types): Full coverage. `context_briefing` and `outcome=1` rows both excluded.
- **R-13** (inferred_edge_count incorrectly counts S1/S2/S8): Partial. Indirect coverage via source-value tests; dedicated unit test absent; xfail integration test present.
- **R-14** (S2 empty vocabulary errors): Full coverage.
- **R-15** (eval gate before rebuild tick): Partial. AC-32 is a manual gate; implementation brief and NFR-08 document the wait requirement.
- **R-16** (file size violation): Full coverage. `wc -l` = 453 ≤ 500 for main module; tests extracted to sibling file (964 lines, separate file per ADR-001).
- **R-17** (validate() zero-value cap): Full coverage. Four separate validate() rejection tests, all pass.

---

### 2. Test Coverage Completeness

**Status**: WARN

**Evidence**:

The RISK-TEST-STRATEGY required 30 scenarios minimum across 17 risks. The implementation provides:
- 36 unit tests in `graph_enrichment_tick_tests.rs` (all pass)
- 17 config tests in `config.rs` (all pass)
- 8 edge-constant tests in `read.rs` (all pass)
- 3 integration tests in `test_lifecycle.py` (1 PASS, 2 XFAIL)

**Gaps acknowledged with rationale**:

| Gap | Risk | Rationale |
|-----|------|-----------|
| No timing test (NFR-03, ≤500ms at ≤1200 entries) | R-04 | Corpus at ~500 entries; query plan uses `idx_entry_tags_tag`; real OOM risk is low. Follow-up required before corpus exceeds 1,200 entries. |
| No explicit crash-simulation watermark test | R-06 | INSERT OR IGNORE idempotency + code review of Phase 5/6 ordering covers the invariant. Risk of silent loss is low. |
| No dedicated `inferred_edge_count` unit test | R-13 | Indirect coverage via source-value assertions is sound; xfail integration test documents the intention. |

All three gaps are non-critical at current corpus size and the underlying code logic is correct per code review.

---

### 3. Specification Compliance

**Status**: PASS

**Evidence**:

All 32 acceptance criteria verified against ACCEPTANCE-MAP.md:

| AC-ID | Status | Notes |
|-------|--------|-------|
| AC-01 through AC-22 | PASS | All S1/S2/S8 behavioral tests pass |
| AC-23 | PASS | Blocking serde-match test passes |
| AC-24 | PASS | All four validate() rejection tests pass |
| AC-25 | PASS | Infallible tick pattern confirmed (warn! on error, no panic) |
| AC-26 | PASS | tick ordering confirmed; integration xfail covers MCP-visible aspect |
| AC-27 | PASS | `grep "graph_enrichment_tick" background.rs` → line 666 (comment), line 790 (call) |
| AC-28 | PASS | `write_graph_edge` at nli_detection.rs:78 |
| AC-29 | PASS | `GraphCohesionMetrics` fields from col-029 unchanged; no new fields added |
| AC-30 | PARTIAL | Source-value tests confirm S1/S2/S8 never write source='nli'; xfail integration test present |
| AC-31 | PASS | 453 lines ≤ 500 |
| AC-32 | DEFERRED | Manual eval gate; requires live server completing at least one tick post-delivery |
| AC-16 | PARTIAL | Idempotency covered; explicit crash-simulation absent |

Non-functional requirements:
- NFR-03 (≤500ms at ≤1200 entries): Not verified by automated test — corpus is currently ~500 entries (WARN)
- NFR-06 (`inferred_edge_count` = NLI-only): Verified via source-value unit tests
- NFR-08 (eval gate after at least one full tick): Documented in AC-32

---

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:

- **Component structure**: `graph_enrichment_tick.rs` (453 lines) + `graph_enrichment_tick_tests.rs` (964 lines, sibling) matches ADR-001 specification exactly.
- **Tick placement**: `run_graph_enrichment_tick` called from `background.rs:790` after `run_graph_inference_tick` — matches architecture tick ordering diagram.
- **tick ordering comment**: Updated at background.rs:666 (AC-27 verified).
- **S8 watermark ordering**: Phase 5 (edge writes) precedes Phase 6 (watermark update) — matches ADR-003.
- **Dual-endpoint quarantine guard**: S1, S2, S8 all JOIN `entries` on BOTH `source_id` and `target_id` with `status = 0` — matches AC-03/AC-08/AC-14 and entry #3981 guard.
- **S2 SQL injection guard**: `push_bind` at all vocabulary term binding sites; SECURITY comment present — matches ADR-002.
- **InferenceConfig dual-site**: Default functions and `impl Default` struct literal match for all 5 fields — matches ADR-005.
- **No new tables**: Confirmed. All writes to existing `graph_edges` and `counters` tables.
- **No new dependencies**: Confirmed. Only sqlx, serde, tracing — already in workspace.
- **SQLITE_MAX_VARIABLE_NUMBER chunking**: Chunked batch IN-clause at 900 params — addresses integration risk from entry #3442.
- **`write_graph_edge` prerequisite**: Confirmed at nli_detection.rs:78 — crt-040 prerequisite gate satisfied.
- **Constants re-exported**: `EDGE_SOURCE_S1/S2/S8` exported from `unimatrix_store::read` and re-exported from `lib.rs` — matches architecture spec.

---

### 5. Integration Smoke Gate

**Status**: PASS

**Evidence**:

`pytest -m smoke --timeout=60`: 22 passed, 0 failed. Gate cleared.

---

### 6. Integration Test xfail Compliance

**Status**: FAIL

**Issue**: Two crt-041 xfail tests do not include a GH Issue reference in their `reason` string, which USAGE-PROTOCOL.md §Failure Triage Protocol requires:

```
Mark the test with @pytest.mark.xfail(reason="GH#NNN")
```

The two tests:
- `test_s1_edges_visible_in_status_after_tick` — xfail reason: "Background tick interval (15 min default) exceeds integration test timeout." No `GH#NNN` reference.
- `test_inferred_edge_count_unchanged_by_s1_s2_s8` — xfail reason: "Background tick interval (15 min default) exceeds integration test timeout." No `GH#NNN` reference.

**Comparison**: The comparable tick-interval xfail test `test_auto_quarantine_after_consecutive_bad_ticks` (line 564) references `GH#291` for the same root cause ("tick interval not overridable at integration level"). The crt-041 xfails should reference the same issue (`GH#291`) since the root infrastructure limitation is identical.

The RISK-COVERAGE-REPORT.md explicitly notes "No GH Issue filed — this is expected infrastructure behavior documented in the test plan." However, USAGE-PROTOCOL.md does not distinguish between bug-type xfails and infrastructure-limitation xfails — both require a GH reference.

**Fix required**: Add `GH#291` reference to both xfail reason strings in `test_lifecycle.py`.

---

### 7. Pre-existing XPASS (crt-040)

**Status**: WARN

**Evidence**: `test_inferred_edge_count_unchanged_by_cosine_supports` (crt-040 test at line 2123) was marked xfail for "no ONNX model in CI" but currently passes. This is pre-existing from crt-040 — the assertion verifies `inferred_edge_count` invariant which does not actually require ONNX-driven writes to pass.

The RISK-COVERAGE-REPORT correctly identifies this as pre-existing and not caused by crt-041. The xfail marker should be removed in a follow-up (it's an XPASS, meaning the test now unexpectedly passes — the marker is stale).

**Assessment**: This is not a crt-041 regression. Not blocking. Should be cleaned up separately.

---

### 8. Knowledge Stewardship

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md includes:
```
## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — found entries #4031, #3822, #4026, #3806, #3935
- Stored: nothing novel to store — [reason given]
```

Both `Queried:` and `Stored:` entries are present with rationale. Compliant.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Two xfail tests lack GH Issue reference | tester (uni-tester) | In `product/test/infra-001/suites/test_lifecycle.py`: update both `@pytest.mark.xfail` decorators at lines 2180 and 2231 to include `GH#291` in the reason string. Example: `reason="GH#291 — Background tick interval (15 min default) exceeds integration test timeout. ..."` |

---

## Post-Rework Scope Note

The xfail fix is a one-line edit per test (adding `GH#291` to the reason string). After the fix, re-validate only Check 6 (integration test xfail compliance). All other checks remain PASS.

## Knowledge Stewardship

- Stored: nothing novel to store — the xfail-without-GH-reference pattern is feature-specific, not a recurring systemic pattern (existing GH#291 references the same infrastructure limitation; future features should reference it). The pattern of checking xfail GH references is already captured in USAGE-PROTOCOL.md.
