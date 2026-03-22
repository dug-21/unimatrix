# Agent Report: crt-025-agent-2-testplan

Phase: Stage 3a (Test Plan Design)
Completed: 2026-03-22

---

## Output Files

All test plan files written to `product/features/crt-025/test-plan/`:

| File | Component | Primary Risks |
|------|-----------|--------------|
| `OVERVIEW.md` | Strategy + integration harness plan | All 14 |
| `validation-layer.md` | Component 1 — infra/validation.rs | R-06, AC-02, AC-03 |
| `mcp-tool-handler.md` | Component 2 — mcp/tools.rs | R-01(partial), R-03, R-08 |
| `hook-path.md` | Component 3 — uds/hook.rs | R-09, AC-16 |
| `session-state.md` | Component 4 — infra/session.rs | R-01(partial), AC-06 |
| `uds-listener.md` | Component 5 — uds/listener.rs | R-01(Critical primary), R-07 |
| `store-layer.md` | Component 6 — store crate | R-02(Critical), R-11, R-14 |
| `schema-migration.md` | Component 7 — migration.rs | R-05, R-10, AC-10, AC-11 |
| `context-store-phase-capture.md` | Component 8 — server.rs/usage.rs | R-01(Critical causal), R-02, R-14 |
| `phase-narrative.md` | Component 9 — unimatrix-observe | R-04, R-08, R-12, R-13 |
| `category-allowlist.md` | Component 10 — infra/categories.rs | R-03, AC-15 |

---

## Risk Coverage Summary

| Risk ID | Priority | Test Location | Coverage |
|---------|----------|--------------|---------|
| R-01 | Critical | uds-listener.md + context-store-phase-capture.md + session-state.md | 4 causal scenarios |
| R-02 | Critical | store-layer.md (drain path) + context-store-phase-capture.md | 3 scenarios incl. pause-advance-flush |
| R-03 | High | category-allowlist.md + infra-001 tools suite | 5 scenarios |
| R-04 | High | phase-narrative.md (build_phase_narrative unit) | 4 threshold scenarios |
| R-05 | High | schema-migration.md | 4 migration integration tests |
| R-06 | High | validation-layer.md | 11 normalization/rejection scenarios |
| R-07 | Medium | uds-listener.md + store-layer.md | 3 seq scenarios |
| R-08 | High | phase-narrative.md (serialization) + infra-001 | 3 scenarios |
| R-09 | Medium | hook-path.md | 3 scenarios (valid, space invalid, empty invalid) |
| R-10 | High | schema-migration.md (fresh DB path) | 3 scenarios |
| R-11 | High | store-layer.md | 2 drain phase value scenarios |
| R-12 | Medium | phase-narrative.md | 2 self-exclusion scenarios |
| R-13 | Medium | phase-narrative.md | 2 orphaned-event scenarios |
| R-14 | High | context-store-phase-capture.md + store-layer.md | 3 call-site scenarios |

---

## Integration Harness Plan Summary

Suites to run in Stage 3c:
- `smoke` — mandatory gate
- `tools` — new `context_cycle` params + `context_store` outcome-category rejection
- `lifecycle` — phase-tag chain end-to-end
- `edge_cases` — phase string boundary cases at MCP level
- `adaptation` — `CategoryAllowlist` change affects existing outcome-related tests

New tests to add to infra-001 in Stage 3c:
- `test_tools.py`: 7 new tests (cycle type, phase_end params, outcome rejection, cycle_review)
- `test_lifecycle.py`: 1 new test (`test_phase_tag_store_cycle_review_flow`)
- `test_adaptation.py`: review + update any `category="outcome"` usages

New Rust test file to create:
- `crates/unimatrix-store/tests/migration_v14_to_v15.rs` (7 tests, follows v13→v14 pattern)

---

## Open Questions

None. All architectural decisions were resolved in ADRs #2998–#3002. The spec and
IMPLEMENTATION-BRIEF had no open questions.

The one note for Stage 3b implementers: the `test_analytics_drain_uses_enqueue_time_phase`
test (R-02) requires the analytics drain to be pauseable in a test context. If the drain
runs automatically on a background task, the test needs either:
(a) a test-only `flush_drain_and_wait()` helper that processes all pending events synchronously, or
(b) the test to be restructured as a direct unit test on the drain handler function
    (passing the `FeatureEntry` variant directly, without the queue).

Option (b) is simpler and is the fallback if (a) is not available. Document the chosen
approach in RISK-COVERAGE-REPORT.md.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") for testing procedures —
  found #229 (Tester duties), #165 (delivery flow), #729 (cross-crate integration pattern),
  #129 (concrete assertions convention). Pattern #729 confirmed that the analytics drain path
  (R-02) must be tested at the store-crate level, not server level.
- Queried: `/uni-knowledge-search` for "session state integration testing validation patterns" —
  found #2125 (analytics drain unsuitable for immediate-read writes), confirming the drain
  test design for R-02.
- Stored: entry #3004 "Analytics drain phase-snapshot causal integration test pattern (crt-025 R-02)"
  via `mcp__unimatrix__context_store`. Novel pattern: three-step pause-advance-flush test design
  for verifying enqueue-time snapshot semantics not previously documented for this codebase.
