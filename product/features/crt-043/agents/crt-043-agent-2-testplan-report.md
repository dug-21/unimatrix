# Agent Report: crt-043-agent-2-testplan

**Phase:** Stage 3a — Test Plan Design
**Component scope:** schema-migration, goal-embedding, phase-capture

---

## Deliverables

| File | Lines | Purpose |
|------|-------|---------|
| `product/features/crt-043/test-plan/OVERVIEW.md` | ~120 | Test strategy, risk-to-test mapping, integration harness plan |
| `product/features/crt-043/test-plan/schema-migration.md` | ~155 | Migration fixture, column presence tests, serialization helper tests, store method tests |
| `product/features/crt-043/test-plan/goal-embedding.md` | ~175 | Embed spawn tests, fire-and-forget timing tests, unavailable service path, MCP regression |
| `product/features/crt-043/test-plan/phase-capture.md` | ~130 | Per-site capture tests, timing contract test, batch binding test |

---

## Risk Coverage Mapping

| Risk ID | Priority | Non-Negotiable | Covered By |
|---------|----------|---------------|------------|
| R-01 | Critical | Yes | EMBED-SRV-01 (await + DB read-back), EMBED-SRV-02 (concurrent stress, `#[ignore]`) |
| R-02 | High | Yes | EMBED-U-01 (round-trip), EMBED-U-02 (malformed blob), EMBED-U-03 (cross-call) |
| R-03 | High | Yes | PHASE-U-01..U-04 (all four write sites), PHASE-U-07, PHASE-U-08 |
| R-04 | High | Yes | PHASE-U-06 (timing: phase = 'design' not 'delivery' after concurrent update) |
| R-05 | High | Yes | MIG-V21-U-03 (real v20 fixture, both columns present), MIG-V21-U-04 (partial apply recovery) |
| R-06 | Med | Yes | MIG-V21-U-05 (re-open v21, no error, schema_version = 21) |
| R-07 | High | No | Code review (embed via ml_inference_pool), EMBED-SRV-07 (< 10ms return) |
| R-08 | Med | No | STORE-U-01 (non-existent cycle_id → Ok, zero rows) |
| R-09 | Low | Yes | EMBED-SRV-03 (empty goal), EMBED-SRV-04 (absent goal) |
| R-10 | High | Yes | EMBED-SRV-05 (EmbedNotReady → warn + NULL), EMBED-SRV-06 (embed error path) |
| R-11 | High | Yes | EMBED-U-01 + EMBED-U-02 (same as R-02; decode helper existence verified by compilation) |
| R-12 | Low | No | EMBED-SRV-09 (MCP response text regression) |
| R-13 | Med | No | Delivery note: written decision before PR (FR-C-07); conditional MIG-V21-U-06 if index added |

---

## Integration Harness Plan Summary

**Suites to run in Stage 3c:**
- `smoke` — mandatory minimum gate
- `lifecycle` — restart persistence regression after v21 migration; also triggers `test_cycle_start_goal_does_not_block_response` (new test)
- `tools` — regression: `context_cycle` response format unchanged (AC-06)

**New tests to write:**
- `product/test/infra-001/suites/test_lifecycle.py::test_cycle_start_goal_does_not_block_response` — validates NFR-01 through the MCP interface (2s budget for full response including binary startup)

**Suites not needed:** volume, security, confidence, contradiction, edge_cases — no changes to search, scoring, security surface, or contradiction detection.

---

## Open Questions for Delivery Agent

1. **v20 fixture builder:** `create_v20_database` must be written as part of `migration_v20_v21.rs`. The v20 DDL is the v19 DDL from `migration_v19_v20.rs` with `schema_version = 20`. No pre-existing `.db` file exists.

2. **Stub EmbedServiceHandle:** Tests for R-01, R-09, R-10 require a configurable stub. The delivery agent must determine whether `EmbedServiceHandle` is directly injectable in tests (e.g., via a trait object or test-feature-gated constructor) and provide the stub. This is the most significant test infrastructure gap.

3. **tracing::warn! capture:** Verify whether `tracing_test` crate is already in the workspace before adding it. Check existing test modules for the established pattern.

4. **Whitespace-only goal:** PHASE-U-09 and EMBED-SRV-03 edge notes flag that spec is silent on `goal = " "`. Delivery agent must decide and document the behavior before the PR is opened.

5. **WARN-2 (pub vs pub(crate) for decode_goal_embedding):** This decision must be made before the PR. If Group 6 will call `decode_goal_embedding` from `unimatrix-server`, it cannot be `pub(crate)`. The test plan reflects `pub(crate)` as the default per ADR-001; the code review assertion must match the actual decision.

6. **ContextSearch write site:** The existence and conditionality of the ContextSearch observation write path must be confirmed. PHASE-U-04 exercises it, but if the path is conditional (only writes when certain conditions hold), the test must set those conditions.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — server unavailable at briefing time, proceeding without.
- Stored: nothing novel to store. The migration fixture builder pattern is already established in `migration_v19_v20.rs`. The fire-and-forget timing test pattern (pre-capture before spawn_blocking) is a first-class application of the existing R-04 risk documentation, not a new pattern.
