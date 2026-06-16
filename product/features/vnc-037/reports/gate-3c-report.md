# Gate 3c Report: vnc-037

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-16
> Result: **PASS**
> Validator: vnc-037-gate-3c

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | RISK-COVERAGE-REPORT.md maps all 20 risks (R-01..R-20) to passing tests; the two recorded gaps (R-13/AC-12, AC-13b) are bounded and ruled below |
| 2. Test coverage completeness | PASS | All Critical/High discriminating scenarios exercised; integration + unit + RED failure paths present and green |
| 3. Specification compliance | PASS | 19 FR implemented & tested; 14 AC verified (AC-12 = accepted deferred-measurement obligation, AC-13 PASS on single-source) |
| 4. Architecture compliance | PASS | 7 ADRs honored incl. AMENDED ADR-005 three-bucket; SQL rank+limit, canonicalization-before-cap-and-count, LEFT JOIN all confirmed in code |
| 5. Knowledge stewardship | PASS | Tester report has `## Knowledge Stewardship` with `Queried:` (MCP disconnected, documented) + `Stored:` with "nothing novel -- {reason}" |
| Integration smoke gate (MANDATORY) | PASS | Independently re-run: **24 passed, 0 failed** (tester reported 23; the +1 is the new vnc-037 smoke test) |
| New suite test_get_edges.py | PASS | Independently re-run: **17 passed, 0 failed** (15 fns; symmetric param ×3) |
| xfail hygiene | PASS | No vnc-037 xfail added; the only edge-relevant xfail (GH#405) confirmed pre-existing + genuinely unrelated |
| No deleted/commented integration tests | PASS | Suites diff vs main is **+547 insertions, 0 deletions** |

## Detailed Findings

### Check 1 — Risk Mitigation Proof
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md maps every risk R-01..R-20 to named tests with results.
The 4 Critical risks are fully covered with **discriminating** (not smoke) scenarios:
- **R-01** (symmetric canon, SR-08 blocker): display AND totals asserted independently across
  Contradicts/CoAccess/Informs; order-of-ops proven (`test_query_ranked_canon_before_cap_authored_wins`).
  Code-confirmed: canonicalization is a CTE (`CASE WHEN relation_type IN ('Contradicts','CoAccess','Informs')`)
  applied **before** the `ORDER BY ... LIMIT` and inside the same `CANON_CTE` the count query reuses
  (`graph_queries_ranked.rs`).
- **R-02** (ranking ORDER BY, SR-09): `test_query_ranked_by_target_confidence_proof_outside_cap`
  seeds the proof target OUTSIDE the cap (#3886) with a *lower* edge weight, proving weight does NOT
  decide. Code ORDER BY confirmed exactly `(d.source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC`.
- **R-03** (split COUNT divergence): three-bucket count + authored tally, canon-parity with the rank
  query proven (`test_count_canon_parity_with_rank_query`).
- **R-04** (rank-and-limit in SQL not Rust, SR-14): proven at the store boundary
  (`test_query_ranked_high_degree_returns_exactly_cap_rows`) + MCP hub test (50 edges → 3 displayed).
All High/Medium/Low risks (R-05..R-20) report Full coverage; spot-verified the load-bearing ones
(R-06 LEFT JOIN, R-07 byte-identity via real producer, R-08 neighbors-suite-unedited, R-14 opt-out,
R-16/AC-14 RED fail-loud).

### Check 2 — Test Coverage Completeness
**Status**: PASS
**Evidence**: Unit tests independently re-run green: `unimatrix-store --lib` **389 passed / 0 failed**,
`unimatrix-server --lib` **4184 passed / 0 failed / 1 ignored** (matches report). Integration:
smoke gate, new suite, lifecycle carry-forward, and the confidence/contradiction regression suites
all independently re-run green (see Integration Validation below). The R-11 trace discipline is
enforced in `graph_queries_ranked_tests.rs` (per-edge rank traces, #3645).

### Check 3 — Specification Compliance
**Status**: PASS
**Evidence**: All 14 AC verified in ACCEPTANCE-MAP + RISK-COVERAGE-REPORT and code-confirmed:
- AC-01..AC-11, AC-14 — PASS (direct test + code evidence).
- AC-03 read-path-only: confirmed **no migration/schema file** on the branch diff vs main.
- AC-12 — **accepted deferred-measurement obligation** (ruling below), not a silent pass.
- AC-13 — PASS on the load-bearing single-source invariant; AC-13b ruling below.
- FR-13/AC-07 byte-identity (SR-01): proven through the real producer (#1268), `entry_to_json`
  signature unchanged, `None ⇒ key absent` structural.
- FR-19/AC-14 fail-loud: handler maps the edge `Err` with the **identical** mapping as the primary
  `entry_store.get` failure and returns via `?` — no degrade-with-note, no silent omit (tools.rs).
No scope additions: per-edge payload remains the exact 5-field discovery shape; no enrichment.

### Check 4 — Architecture Compliance
**Status**: PASS
**Evidence**: All 7 ADRs honored (independently code-verified):
- ADR-001/006/007 — SQL rank+limit + separate split COUNT; ranked query lives in a **new** sibling
  `graph_queries_ranked.rs` (255 lines), plain `query_direct_neighbors` gains only the `source` column.
- ADR-002 — discovery-list projection preserved (no enrichment field added).
- ADR-003 — serializer seam unchanged; `None ⇒ key absent`.
- ADR-004 — additive `source` column, no DDL; neighbors suite passes UNEDITED.
- **ADR-005 (AMENDED three-bucket)** — `EdgeCountSplit { inbound, outbound, both, authored }` and
  `EdgeTotals { inbound, outbound, both }` confirmed; `↔` counts once in `both` (lines 215-218 of
  the count SQL). Markdown sub-split dropped.
- ADR-006 — `GET_EDGE_DISPLAY_LIMIT: i64 = 3` single constant in `read.rs:1881`, re-exported from
  `lib.rs`, bound via `LIMIT ?2` (never a literal 3); render derives `n = total - cap`.
- ADR-007 — symmetric canonicalization before rank AND count.
Security: all queries use positional binds (`?1` anchor, `?2` cap, IN-list placeholders); canon
`CASE`/`ORDER BY`/`LIMIT` are static SQL — no injection surface. No `.unwrap()`/`.expect()` on the
edge path (doc-comment mentions only). All new/touched edge files ≤500 lines (largest new = 346).

### Check 5 — Knowledge Stewardship
**Status**: PASS
**Evidence**: `vnc-037-agent-4-tester-report.md` contains a `## Knowledge Stewardship` block with
`Queried:` (context_briefing — MCP disconnected, documented as non-blocking per spawn note; applied
#3886 + #1268 from prior queries) and `Stored:` ("nothing novel to store -- {reason given}"). A
reason follows the "nothing novel" — not even a WARN.

## Integration Test Validation (MANDATORY)

| Item | Result | Evidence |
|------|--------|----------|
| Smoke gate passed | PASS | **Independently re-run: 24 passed, 0 failed, 0 xfailed** (tester reported 23; the new vnc-037 smoke test in test_get_edges.py is the +1 — an increase, not a regression) |
| New suite test_get_edges.py exists + run | PASS | File present (`product/test/infra-001/suites/test_get_edges.py`, 15 fns, symmetric param ×3); **independently re-run: 17 passed, 0 failed** |
| Relevant suites run | PASS | Independently re-ran carry-forward lifecycle test + confidence + contradiction: **27 passed, 1 xfailed** (xfail = GH#405) |
| xfail markers have GH Issues | PASS | No vnc-037 xfail added. All xfails in the harness are pre-existing (GH#111, GH#405, GH#406, env/tick). |
| GH#405 genuinely unrelated | CONFIRMED | `test_base_score_deprecated` / `test_deprecated_visible_in_search` concern background confidence-scoring **timing** (deprecated vs active base score). vnc-037 only *reads* `entries.confidence` as a LEFT-JOIN rank key; it never writes/mutates confidence. Not masking any feature bug; predates vnc-037 (col-028 era). |
| No integration tests deleted/commented | PASS | `git diff main..feature/vnc-037 -- suites/` = **+547 insertions, 0 deletions**. No `-def test_`, no `-@pytest.mark.xfail`. |
| RISK-COVERAGE-REPORT includes integration counts | PASS | Report's Test Results section tabulates smoke (23), test_get_edges (17), lifecycle (2), regression suites, with per-suite selection rationale. |

Note on the report's "4 initial integration failures": correctly triaged as **test-seeding bugs**
(MCP semantic store-dedup collapsing near-identical seeded entries to one id), fixed by seeding
target rows via direct SQL. The corresponding store/server unit tests for the same behaviors were
already green, ruling out a feature defect. No feature code changed; no tests deleted. Sound triage.

Note on the moved tests: `entries.rs` lost 141 lines incl. 8 `test_response_format_*` functions —
these were **moved** to `response/mod.rs` (verified present + green), a legitimate OQ-B file-size
refactor (entries.rs now 376 lines). No coverage lost.

## Ruling: AC-12 (latency baseline)

**RULING: NOT a gate blocker — accepted deferred-measurement obligation.**

The spec is explicit (C-9 / OQ-C / NFR-2): the proposed ≤5 ms p50 / ≤15 ms p95 numbers are
**provisional until measured**, and if unattainable the choice (relax budget / mandate OQ-03
opt-out / revisit default-on) is a **human decision**, not an implementation defect. The
structural mechanism that makes the budget reachable — rank-and-limit-in-SQL bounding hub
fan-out to 3 rows + scalar counts — **is proven** (`test_query_ranked_high_degree_returns_exactly_cap_rows`
at the store boundary; `test_get_high_degree_node_caps_at_three` at the MCP boundary; read-pool +
indexed-JOIN design confirmed). What is unproven is only the *measured number lock*, which the spec
deliberately gates on a human soft-decision and which this environment cannot produce (no
representative high-degree perf store). The report records AC-12 as OPEN honestly rather than
silently passing it. This is the spec's designed outcome, not a coverage miss.

**Action for the human (carry-forward, non-blocking):** before locking the AC-12 numbers in a
production posture, run the edge-free vs default-on baseline on a representative store with a
high-degree node, and choose per C-9 if the budget proves unattainable.

## Ruling: AC-13b (cap-isolation override variant)

**RULING: AC-13 is adequately satisfied. AC-13b's runtime-override variant is a structural
impossibility for a compile-time `const`, not a coverage gap.**

The load-bearing half — **AC-13a single-source** — is fully proven: `GET_EDGE_DISPLAY_LIMIT`
is one named `const` (`read.rs:1881`), the SQL binds `LIMIT ?2` to it (no literal 3), the render
derives `n = total − cap` with `cap = GET_EDGE_DISPLAY_LIMIT` (`edges_render.rs:77-79`), and tests
reference the constant, not `3` (grep + `test_query_ranked_no_literal_three_and_positional_binds`,
`test_markdown_capped_pointer_references_constant`). A one-line edit to the constant is provably the
only change needed to retune the cap. The AC-13b "override the constant at runtime, assert only the
rendered set shrinks" variant cannot be expressed for a compile-time `const` without adding a
test-only feature flag — which would be a scope addition the spec does not authorize. The intent of
AC-13b (cap value is decoupled from totals + canonicalization) is structurally guaranteed: totals
are computed by an **uncapped** count query that never references `GET_EDGE_DISPLAY_LIMIT`, and
canonicalization happens in the CANON_CTE independent of the cap. AC-13 is PASS.

## Rework Required

None.

## Scope Concerns

None. Two open items are spec-designed human soft-decisions (AC-12 number lock) and a structural
const limitation (AC-13b), neither of which blocks the gate.
