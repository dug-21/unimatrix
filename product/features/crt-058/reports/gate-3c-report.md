# Gate 3c Report: crt-058

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-07-05
> Result: PASS
> Validated against committed HEAD `82c280c5` (branch `feature/crt-058`)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (R-01…R-11) | PASS | Every risk maps to a passing behavioral test; independently re-ran the subset unit invariant + 7 integration tests + full server crate |
| 2. Test coverage completeness | PASS | All risk-to-scenario mappings exercised; cross-component risks covered through the real handlers; edge cases (self-loop, high-degree, zero, concurrent-tick, idempotent) present |
| 3. Specification compliance | PASS | AC-01…AC-11 verified; FR-01…FR-09, NFR-01…NFR-06 addressed; no scope additions |
| 4. Architecture compliance | PASS | Step-6.5 placement, LOCKED predicate, write-pool, non-fatal semantics, chokepoint-only, tick unchanged — all match ARCHITECTURE.md + ADR-001..004 |
| 5. Knowledge stewardship | PASS | Tester report carries `## Knowledge Stewardship` with `Queried:` + `Stored: #5470` |
| Integration smoke gate (MANDATORY) | PASS | `pytest -m smoke` → 28 passed, 0 failed (re-run, 231s) |
| 3 Gate-3b-deferred verifications | PASS | AC-10 / AC-06 / AC-07 present AND passing through the REAL handlers (re-run) |

**Independent verification performed** (not just report-reading):
- `cargo build --release` → clean.
- 7 crt-058 integration tests → **7 passed** (61s).
- `pytest -m smoke` → **28 passed** (231s).
- crt-058 unit subset (delete_agent / edge_cleanup / subset_of_tick / successor_bearing / edges_removed) → **20 passed**.
- `cargo test -p unimatrix-server --lib` → **4398 passed, 0 failed**.

## Detailed Findings

### Check 1 — Risk mitigation proof (R-01…R-11)
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md maps all eleven risks to named passing tests. Independently confirmed the load-bearing ones:
- **R-01 (Critical — subset blind spot)**: unit `test_deprecate_eager_subset_of_tick_and_exactly_agent_edges` (both real fns, R⊆T AND R==2 agent edges) + `test_successor_bearing_edge_repointed_by_tick_but_eager_would_destroy` (negative-mutation, documents the hazard) + **integration `test_correct_successor_never_invokes_eager_cleanup`** (real `context_correct` → no `edge_cleanup` audit, inbound agent edge survives). The successor-less blind spot is closed by the chokepoint-exclusion assertion against the real handler, exactly as the Risk Strategy required.
- **R-03 (post-commit atomicity)**: `delete_agent_edges_for_entry` is a single `DELETE … RETURNING` via one `fetch_all`; count = `returned.len()`, the same tuples that feed the audit — no delete-then-select window. Design-closed.
- **R-11 (idempotency/ordering)**: step 6.5 sits after the step-5 early-return and after the step-6 flip; the early-return passes `None`. Integration `test_redeprecate_idempotent_no_second_cleanup_audit` confirms no second delete/audit.

### Check 2 — Test coverage completeness
**Status**: PASS
**Evidence**: Per-source matrix (agent removed, `co_access`/machine survive), self-loop-counted-once, high-degree, concurrent-tick zero-row tolerance, and double-audit distinctness are unit-covered; the MCP-visible chain (removal + count + audit tuples + non-fatal + idempotent + zero-case) is integration-covered through the real wire. RISK-COVERAGE-REPORT includes integration counts (smoke 28; 7 new feature tests; touched-surface regression: test_tools 32 passed/1 xfail, test_edge_cases 24 passed/1 xfail, test_protocol 13 passed).

### Check 3 — Specification compliance
**Status**: PASS
**Evidence**: ACCEPTANCE-MAP AC-01…AC-11 all traced to state/parse-based verifications; re-ran the deferred three. AC-05 renders literal `0` (`result.parsed["edges_removed"] == 0`); AC-06 omits the key entirely (`None ≠ Some(0)`); AC-02/AC-04 parse the Json integer, not a substring. No unrequested features; NOT-in-scope exclusions (no relation-type filter, machine edges untouched, no tick change, correct path excluded) all hold in code.

### Check 4 — Architecture compliance
**Status**: PASS
**Evidence**: `tools.rs` step 6.5 matches the architecture data-flow byte-for-intent: after flip, non-fatal `match`, `Some(tuples.len())` incl. `Some(0)`, `Err → warn!(entry, error) + None`, audit only when `!tuples.is_empty()`. Predicate is verbatim the LOCKED clause `WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2 RETURNING …` on `write_pool_server()`; `?2` bound to the `EDGE_SOURCE_AGENT` constant (not user input). Audit `operation = "context_deprecate.edge_cleanup"`, `target_ids = [entry]`, metadata serialized via `serde_json::to_string` (serialization failure warns and returns — no panic, no `"{}"` fallthrough on non-empty). `run_orphaned_edge_compaction` production code is UNCHANGED (background.rs diff is entirely `#[cfg(test)]` — a shared test helper relocation plus the subset/repoint test modules). AC-08 holds.

### Check 5 — Knowledge stewardship
**Status**: PASS
**Evidence**: `crt-058-agent-6-tester-report.md` → `## Knowledge Stewardship` with `Queried:` (context_briefing → #3806/#3386/#5460/#2758) and `Stored: #5470` (SQLite BEFORE DELETE trigger technique for driving a non-unit-constructible handler's swallowed-failure path).

## Gate-3b-deferred verifications (mandatory confirmation)

All three are present as integration tests through the REAL handlers and passed on re-run:

1. **AC-10 chokepoint-exclusion** (`test_correct_successor_never_invokes_eager_cleanup`): real `context_correct`; asserts NO `context_deprecate.edge_cleanup` audit for the original, and the inbound agent edge survives. **The relaxed assertion ("exactly one agent edge from the source survives") is a correct reflection of crt-058's guarantee, not a weakening.** crt-058 guarantees only that the eager helper never destroys a repointable edge. vnc-017's auto-redirect synchronously repoints `a→e` to `a→successor`, so the edge persists (count of agent edges from `a` == 1). Binding the assertion to the exact successor target would couple the test to vnc-017's concern, which is out of crt-058 scope; asserting "not eagerly deleted / exactly one survives" is the precise crt-058 property. No bug hidden.
2. **AC-06 non-fatal injected failure** (`test_deprecate_eager_failure_is_non_fatal`): a `BEFORE DELETE … RAISE(ABORT)` trigger forces the real eager `DELETE … RETURNING` to `Err`; asserts success + `deprecated: true` + `warn "eager edge cleanup failed"` carrying the entry id + advisory OMITTED (`edges_removed` key absent, distinct from `Some(0)`) + agent edges REMAIN + no `edge_cleanup` audit.
3. **AC-07 idempotency** (`test_redeprecate_idempotent_no_second_cleanup_audit`): real re-deprecation; second call omits the advisory (`None`), the fresh post-deprecation agent edge survives, and no second `edge_cleanup` audit is written.

## Integration test hygiene

- **Smoke**: 28 passed, 0 failed (re-run).
- **xfail hygiene**: two pre-existing xfails encountered in touched suites, each with a GH Issue and unrelated to crt-058:
  - `test_tools.py::test_deprecated_visible_in_search_with_lower_confidence` — GH#405 (deprecated-vs-active confidence under background scoring timing). crt-058 touches neither confidence scoring nor search ranking — **genuinely unrelated**, not masking a feature bug.
  - `test_lifecycle.py` GH#406 / GH#291 (multi-hop injection; tick-interval override) — untouched, unrelated.
- **No integration tests deleted or commented out** — the three suite diffs are pure additions.
- **RISK-COVERAGE-REPORT includes integration test counts** — yes (smoke + 7 feature + regression tallies).

## Rework Required

None.

## Scope Concerns

None.
