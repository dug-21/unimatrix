# Gate 3c Report: vnc-044

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-07-05
> Result: REWORKABLE FAIL
> Validator: vnc-044-gate-3c

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (R-01..R-13) | PASS | Every risk maps to passing coverage; R-08 also has deterministic unit coverage independent of the flaky integration test below. |
| 2. Test coverage completeness | **FAIL** | AC-07 integration test `test_graph_legacy_summary_alias_equivalent` is flaky (~50% under load, forced to fail under CPU load). It asserts byte-identical output over a payload containing `confidence`, a field the server's background scoring mutates between the two sequential wire calls. Coverage is unreliable, and RISK-COVERAGE-REPORT overstates stability. |
| 3. Specification compliance (FR/AC) | PASS | AC-02..AC-09 satisfied by code. AC-07 *behavior* is correct (payloads byte-identical modulo the background-mutated field). AC-04 fallback accepted (see finding). |
| 4. Architecture compliance (ADR-001/002) | PASS | Resolver-before-dispatch, distinct projection type/module, shared types untouched, seam threading all match ADR-002. |
| 5. Knowledge stewardship (tester) | PASS | `## Knowledge Stewardship` present with `Queried:` (context_briefing) + `Stored:` (#5521) entries. |

**Mandatory integration gates (validator-run, not just reported):**
- Smoke (`pytest -m smoke`): **30 passed** — re-run and confirmed (248s).
- vnc-044 graph integration (`-k graph`, test_tools + test_lifecycle): **63 passed / 1 failed** — the 1 failure is the flaky AC-07 test (Check 2).
- No new `xfail` markers for vnc-044 confirmed (all xfails in suites reference GH#405/#406/ONNX-tick/col-028 — none vnc-044). No integration tests deleted/commented.

## Detailed Findings

### Check 1 — Risk mitigation proof
**Status**: PASS
Every risk R-01..R-13 in RISK-TEST-STRATEGY.md maps to executed tests. Spot-verified:
- R-01/R-02 (UTF-8 flooring + truncated byte-compare, Critical): `verbosity.rs` carries the 10-case boundary table incl. exactly-256/257-ASCII/2-3-4-byte straddle/boundary-exact + `test_content_truncated_257_ascii_true` (the false-negative trap) + arbitrary-unicode no-panic fuzz. `content_preview` uses the mandated idiom (`while end>0 && !is_char_boundary(end)`), `truncated == content.len() > CONTENT_PREVIEW_BYTES`. Correct.
- R-03 (default-summary all 5 node-bearing modes + per-envelope metadata): `graph_read_projection.rs` impls preserve `truncated`/`seed_ids`/`depth_reached` (subgraph), `Truncated{forward,backward}` (chain), single-node object (current — R-03 shape trap), `total_returned` (inverse/filter). Unit + integration coverage present.
- R-04 (full byte-identical): the `Detail::Full` arm is `serde_json::to_string(&result)` on the **unchanged** typed envelope; `EntryRecord`/`EdgeRecord` UNTOUCHED. No-regression is structural, not just tested (see AC-04).
- R-05 (markdown reject 7 modes, pre-dispatch): `resolve_graph_output` runs at top of `handle_graph`; integration `test_graph_markdown_rejected_all_modes[7 modes]` in the 63-passed set.
- R-06 (shared types untouched): #878 link smoke rc=0 (per report); code review confirms no `skip_serializing_if` added, projection is a distinct `NodeSummary`/`serde_json::Value`.
- R-08 (legacy alias + conflict): deterministic unit resolver tests (`test_resolve_legacy_summary_alias`, `_conflict`) + `test_graph_legacy_summary_conflict_rejected` pass. The alias-equivalence integration test flakes (Check 2) but R-08 is independently mitigated at the unit level.
- R-11 (lifecycle-vs-delivery): carried-by-design, documented in tool description + tests as an illustration, not a defect. Correct per mandate.

### Check 2 — Test coverage completeness
**Status**: FAIL (REWORKABLE, test-only)
**Evidence**: `suites/test_tools.py::test_graph_legacy_summary_alias_equivalent` (AC-07 / R-08) asserts the two sequential `context_graph` calls (`format=summary` alias vs `format=json,detail=summary`) produce byte-identical wire text. Observed behavior:
- Passes in isolation on an idle machine; fails ~50% under load; forced to fail 100% under injected CPU load.
- Captured diff: the two payloads are byte-for-byte identical **except node id:4's `confidence`** — first call read `0.464`, second read `0.47000000000000003`. The server's background adaptation/scoring (the documented GH#405 "background scoring timing" dynamic) mutated `confidence` in the window between the two calls. `confidence` is one of the 8 summary fields, so it rides in the wire payload.

**Interpretation**: This is a **test-robustness defect, not a production defect**. The legacy-alias resolution is provably correct — the two code paths yield identical serialization; the byte diff is a single field mutated by a background process between two live reads. But as shipped the test will false-red intermittently in CI, and the RISK-COVERAGE-REPORT's claims ("26 new vnc-044 integration tests, all PASS", "no flakes", Gaps: "None") are inaccurate for this test.

**Issue / fix**: uni-tester must make the AC-07 byte-equality assertion robust to the background-mutable `confidence` field — e.g., parse both JSON payloads and compare structurally with `confidence` normalized/excluded, or otherwise remove the two-sequential-reads race (a single read reused, or freeze scoring). The deterministic unit resolver test already pins the alias→detail mapping, so AC-07 *behavior* is not in question — this is a narrow test-assertion rework. Also correct RISK-COVERAGE-REPORT's stability/"no flakes" claim.

### Check 3 — Specification compliance
**Status**: PASS
- AC-02 (both axes end-to-end): `resolve_graph_output` + `serialize_detail` threaded into all 5 node-bearing arms; `neighbors`/`path` use plain `to_string` (accept-and-ignore). Smoke `test_graph_detail_axis_threaded` passed.
- AC-03/AC-03b: exact 8-field node / 4-field edge with present-AND-absent-key unit assertions; UTF-8 preview boundary table. Correct.
- AC-04 (full byte-for-byte): **fallback method accepted.** The `Detail::Full` arm is `serde_json::to_string(&result)` over the untouched typed envelopes; `EntryRecord`/`EdgeRecord`/the five envelopes are UNTOUCHED (confirmed in code + ARCHITECTURE integration table). No-regression is therefore guaranteed *by construction* — the full path is the pre-vnc-044 serialization of unchanged types. The plan's fallback (complete `EntryRecord` key set present + byte-stability across identical runs) adequately confirms this given a true pre-change binary golden is impractical in-harness. AC-04's byte-for-byte intent is met.
- AC-05/AC-06/AC-08/AC-09: covered by the 63-passed graph integration set + unit tests + tool-description review.
- AC-07: code compliant; the *test* is flaky (Check 2).

### Check 4 — Architecture compliance
**Status**: PASS
Implementation matches ADR-001/ADR-002 and ARCHITECTURE.md: `resolve_graph_output` at top of `handle_graph` before dispatch (uniform rejection); graph does not call shared `parse_format`; `GraphSerialization{Json}` single variant with markdown rejected before production; `NodeSummary` in dedicated `graph_read_projection.rs` (430 lines) with `GraphSummaryProjection` trait for the 5 node-bearing envelopes; shared primitives (`Detail`, `parse_detail`, `CONTENT_PREVIEW_BYTES`, `content_preview`) single-sourced in `response/verbosity.rs` (371 lines); `EntryRecord`/`EdgeRecord`/`ResponseFormat`/`parse_format` untouched; `GraphParams.detail` additive `Option<String>`. `graph_read.rs` = 469 lines (< 500). No architectural drift.

### Check 5 — Knowledge stewardship (test-phase)
**Status**: PASS
`vnc-044-agent-6-tester-report.md` contains a `## Knowledge Stewardship` block with `Queried:` (context_briefing surfacing #5510/#5509/#4502/#4503/#4490/#5389) and `Stored:` (#5521, a fixture/dedup migration-trap pattern). Obligations met.

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| AC-07 integration test `test_graph_legacy_summary_alias_equivalent` flakes ~50% (100% under load) because it byte-compares two sequential reads whose summary payload includes background-mutable `confidence` | uni-tester | Make the assertion robust to `confidence` drift — parse both payloads and compare structurally with `confidence` normalized/excluded, or eliminate the two-sequential-reads race. Do NOT weaken the AC-07 field-set coverage. Then re-run `-k graph` to confirm stability. |
| RISK-COVERAGE-REPORT.md claims "no flakes" / Gaps "None" while a delivered test flakes | uni-tester | Correct the report's stability claim for AC-07 / R-08 (unit coverage is deterministic; the integration byte-equality assertion was fragile). |

## Notes (not defects)

- R-11 lifecycle-vs-delivery-status gap is carried-by-design (named follow-up #3); documented in the tool description and asserted as an illustration. Not failed.
- The core feature CODE is complete and correct — this REWORKABLE FAIL is narrow test-robustness + report-accuracy rework. Behavior for all AC-02..AC-09 is proven correct (AC-07 payloads are byte-identical modulo the background-mutated field). A reviewer could reasonably accept this as a WARN, but a coin-flip-flaky test entering the shared suite must be addressed.

## Knowledge Stewardship
- Stored: nothing novel to store -- the flaky-test root cause (byte-comparing a payload that carries a background-mutable field, GH#405 confidence-scoring dynamic) is a feature-specific test defect, not a recurring cross-feature validation pattern; filing it as knowledge would poison recall per the "bugs are GH issues, not lessons" rule. Routed as gate rework to uni-tester instead.
