# Agent Report: crt-042-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Feature**: crt-042 (PPR Expander)
**Agent ID**: crt-042-agent-2-testplan

---

## Deliverables

All test plan files produced in `product/features/crt-042/test-plan/`:

| File | Lines | Coverage |
|------|-------|---------|
| `OVERVIEW.md` | 232 | Risk-to-test mapping (all 17 risks), integration harness plan, AC-00 SQL plan, R-08 grep plan, R-04/AC-24 tracing design, AC-25 design |
| `graph_expand.md` | 506 | 21 unit tests + 1 grep check for Component 1 |
| `phase0_search.md` | 345 | ~10 unit/integration tests + 2 infra-001 tests for Component 2 |
| `inference_config.md` | 368 | 16 unit tests + 1 mandatory grep check for Component 3 |
| `eval_profile.md` | 195 | 3 shell checks + 4 eval measurements + 1 procedural gate for Component 4 |

---

## Risk Coverage Mapping Summary

| Priority | Count | Key tests |
|----------|-------|-----------|
| Critical | R-01, R-02 | Smoke suite (R-01); AC-00 SQL gate + unidirectional fixture pair (R-02) |
| High | R-03, R-04, R-05, R-07, R-08, R-16 | AC-13/14 quarantine; AC-24 tracing subscriber; combined ceiling test; AC-25 cross-category; grep scan + 16 config tests; AC-02 insertion point |
| Med | R-09–R-14, R-17 | AC-16 grep; cycle termination; seed exclusion; determinism; 4 validation tests; S8 unidirectional fixture |
| Low | R-15 | AC-15 None embedding skip |

**Non-negotiable tests confirmed in plans**:
- AC-01 flag-off regression: infra-001 smoke suite + dedicated pool-size unit test
- AC-14 quarantine bypass: infra-001 lifecycle test with graph-reachable quarantined entry
- AC-24 timing instrumentation: tracing subscriber test (NOT deferred — entry #3935 lesson applied)
- AC-25 cross-category behavioral proof: new lifecycle test mandatory regardless of eval gate outcome
- AC-18/19/20/21 config validation: four tests with ppr_expander_enabled=false proving unconditional enforcement

---

## Integration Suite Plan

| Suite | Action |
|-------|--------|
| `smoke` | MANDATORY gate — run first; covers R-01 flag-off regression |
| `tools` | Run — existing search tests form the R-01 regression baseline |
| `lifecycle` | Run + new test — AC-14 quarantine bypass test; AC-25 cross-category test |
| `security` | Run — existing quarantine exclusion tests; validates R-03 at integration level |
| Others | Not required — no schema change, no confidence change, no contradiction change |

**New tests to add to infra-001**:
1. `test_search_graph_expand_excludes_quarantined_entry` → `suites/test_lifecycle.py` (AC-14)
2. `test_search_graph_expand_surfaces_cross_category_entry` → `suites/test_lifecycle.py` (AC-25)

---

## Open Questions

1. **AC-25 server config override**: The MCP harness does not obviously support per-test server config overrides. If `ppr_expander_enabled` cannot be set per test in infra-001, AC-25 must be implemented as a Rust `#[tokio::test]` in `unimatrix-server/tests/` rather than the MCP harness. Delivery agent must confirm the harness fixture capabilities before Stage 3b.

2. **tracing-test crate availability**: The tracing subscriber test (AC-24) recommends `tracing-test = "0.2"` as a dev-dependency. If this crate is not already in `unimatrix-server/Cargo.toml`, the delivery agent must add it. Alternatively, a manual `tracing_subscriber::fmt::TestWriter` approach works without the crate.

3. **SearchService constructor for unit tests**: Phase 0 unit tests require a `SearchService` instance with controlled `ppr_expander_enabled`. Confirm whether `SearchService::new()` is directly constructible in unit tests or requires a mock/test-builder pattern.

4. **S1/S2 back-fill issue number**: AC-00 requires a back-fill GH issue to be filed if Informs edges are single-direction. The test plans reference this as a gate but cannot confirm the issue number until AC-00 is executed by the delivery agent.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 17 ranked entries returned. Most relevant: #3754 (direction semantics lesson), #4049–#4054 (all crt-042 ADRs), #4044/#2730 (InferenceConfig hidden test sites), #3631 (inline test placement), #3740 (submodule pattern).
- Queried: `context_search` × 4 — confirmed no duplicate patterns for the BFS fixture pair technique.
- Queried: `context_get` × 6 — full content of all crt-042 ADRs and entry #3754 retrieved.
- Stored: entry #4066 "BFS graph expand test plan: pair behavioral direction test with unidirectional fixture to document back-fill dependency" via `context_store` (novel pattern — the before/after back-fill fixture pair is not previously documented for BFS functions).
