# Agent Report: crt-046-agent-3-risk

## Output

- **RISK-TEST-STRATEGY.md**: `product/features/crt-046/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count | Risks |
|----------|-------|-------|
| Critical | 5 | R-01 (memoisation gate), R-02 (write_graph_edge contract), R-03 (drain shedding), R-04 (parse-failure observability), R-05 (migration cascade) |
| High | 7 | R-06 (partial cluster row), R-07 (recency cap), R-08 (NULL short-circuit), R-09 (pair cap timing), R-10 (bidirectional edge), R-11 (cold-start regression), R-12 (inactive entry leakage) |
| Med | 3 | R-13 (zero-slot suppression), R-14 (spawn_blocking), R-15 (DDL mismatch) |
| Low | 1 | R-16 (outcome weight boundary) |
| **Total** | **16** | |

## Top Risks for Human Attention

1. **R-01 (Critical)** — Memoisation gate bypass: the `force=false` early-return must be placed AFTER the step 8b call site, not before. This is an ordering constraint in `mcp/tools.rs` that is easy to get wrong and invisible without AC-15.

2. **R-02 (Critical)** — `write_graph_edge` return contract: counters must key off `rows_affected() > 0 == true`, not `Ok(_)`. Root cause of crt-040 Gate 3a rework (entry #4041). The pseudocode must lead with the three-case contract table before any implementation prose.

3. **R-05 (Critical)** — Migration cascade: 9 touchpoints from entry #3894 enumerated in ARCHITECTURE.md. Gate 3a is a FAIL if the cascade grep (`grep -r 'schema_version.*== 21' crates/`) returns non-zero. High likelihood because this has been missed in prior features (crt-033, crt-035).

4. **R-04 (Critical)** — Parse-failure observability: `parse_failure_count` must appear in the MCP response payload, not only server logs (FR-03 / AC-13). The architecture currently logs at `warn!` only — the spec adds the response field. Implementation must not omit this.

5. **R-03 (Critical)** — Drain flush in integration tests: all tests asserting `graph_edges` after `context_cycle_review` must synchronize with the analytics drain before asserting. Tests that skip this will be intermittently flaky (new pattern #4114).

## Coverage Gaps

- **AC-13** (parse_failure_count in MCP response) has no precedent in existing tests — this is a new response field that requires the tester to inspect the raw MCP payload, not just side-effect DB state.
- **AC-15** (force=false step 8b re-emission) is the only test that catches R-01; no other test detects the bypass.
- **E-02** (self-pair A→A deduplication) is unspecified in the SCOPE and SPECIFICATION — the tester must add a scenario but should confirm expected behavior with the architect first.
- **I-04** (empty `current_goal` at briefing time) — spec guards on feature absent, not on empty goal; the implementation must check both and tests must cover the empty-goal case explicitly.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection` — found entries #2758, #4043, #4076 (gate failures related to test omission and pending-decision language; informed R-01/R-05 severity elevation).
- Queried: `/uni-knowledge-search` for `write_graph_edge return contract` — found entry #4041 (direct evidence for R-02 Critical rating).
- Queried: `/uni-knowledge-search` for `schema migration cascade` — found entry #3894 (definitive cascade checklist applied to R-05).
- Queried: `/uni-knowledge-search` for `analytics drain shedding` — found entries #2148, #4108 (drain timing pattern; informed R-03 and integration risk I-02).
- Stored: entry #4114 "Force analytics drain flush before asserting graph_edges in integration tests using enqueue_analytics" via `context_store` — novel pattern not previously captured, visible across crt-046 and any future feature using enqueue_analytics for graph edges.
