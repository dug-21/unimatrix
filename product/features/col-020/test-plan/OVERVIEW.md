# col-020 Test Plan Overview

## Test Strategy

Three test layers, ordered by execution cost:

1. **Unit tests** -- Pure computation in `unimatrix-observe` (C1 session_metrics, C2 types) and `unimatrix-store` (C4 batch queries, counter setter). No server, no MCP.
2. **Integration tests** -- Server-side knowledge reuse (C3) and handler orchestration (C6) require Store setup. Tested via `#[tokio::test]` with real Store instances.
3. **MCP integration tests** -- infra-001 harness exercises the compiled binary end-to-end.

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test Layer | Test Plan File |
|---------|----------|-------------|-----------|----------------|
| R-01 | High | C3, C4 | Unit + Integration | knowledge_reuse.md, store_api.md |
| R-02 | High | C3, C6 | Integration | knowledge_reuse.md, handler_integration.md |
| R-03 | High | C6 | Integration | handler_integration.md |
| R-04 | High | C3 | Integration | knowledge_reuse.md |
| R-05 | High | C4, C6 | Integration | store_api.md, handler_integration.md |
| R-06 | Med | C1 | Unit | session_metrics.md |
| R-07 | Med | C1 | Unit | session_metrics.md |
| R-08 | Med | C6 | Unit | handler_integration.md |
| R-09 | High | C2 | Unit | types.md |
| R-10 | High | C1, C3, C6 | Unit + Integration | session_metrics.md, knowledge_reuse.md, handler_integration.md |
| R-11 | Low | C4 | Unit | store_api.md |
| R-12 | Med | C3 | Integration | knowledge_reuse.md |
| R-13 | Med | C1 | Unit | session_metrics.md |
| R-14 | High | C6 | Integration | handler_integration.md |
| R-15 | Low | C1 | Unit | session_metrics.md |

## Cross-Component Test Dependencies

- C1 (session_metrics) is pure computation -- no dependencies, testable in isolation.
- C2 (types) defines structs consumed by C1, C3, C5, C6 -- tested via serde round-trips.
- C3 (knowledge_reuse) depends on C4 (Store batch APIs) -- integration tests seed Store then compute reuse.
- C4 (store_api) depends on existing Store infrastructure -- tested with real SQLite.
- C5 (report_builder) has no code changes (post-build mutation pattern) -- no dedicated tests needed.
- C6 (handler_integration) orchestrates C1, C3, C4 -- integration tests validate the full pipeline.

## Integration Harness Plan (infra-001)

### Suites to Run

This feature touches server tool logic (context_retrospective handler) and store/retrieval behavior. Per the suite selection table:

| Suite | Reason |
|-------|--------|
| `smoke` | **Mandatory gate** -- minimum regression baseline |
| `tools` | context_retrospective is a tool; parameter and response validation |
| `lifecycle` | Multi-step flows validate that retrospective still works end-to-end |

### Existing Coverage Assessment

- `test_tools.py` has existing retrospective tests validating the basic flow (call with feature_cycle, get report back). These cover the existing pipeline but do NOT validate the new optional fields (session_summaries, knowledge_reuse, rework_session_count, context_reload_pct, attribution).
- `test_lifecycle.py` exercises store-then-retrospective flows. New fields will appear in the response but are not asserted on.

### Gaps in Existing Suites

The new col-020 behavior is primarily **additive optional fields** on an existing response. The existing integration tests will continue passing because:
- New fields are `Option` with `serde(default, skip_serializing_if)` -- old assertions still hold.
- No existing behavior is changed.

However, there is no existing integration test that verifies:
1. Session summaries appear in retrospective output when observation data spans multiple sessions.
2. Knowledge reuse fields populate when cross-session query_log/injection_log data exists.
3. topic_deliveries counters update after retrospective runs.

### New Integration Tests to Add

**Suite: `test_lifecycle.py`** (multi-step flows are the natural home):

```python
def test_retrospective_session_summaries(shared_server):
    """Store observations across 2 sessions, run retrospective, verify
    session_summaries field contains 2 entries with correct tool distributions."""

def test_retrospective_knowledge_reuse(shared_server):
    """Store entry in session A, search returning that entry in session B,
    run retrospective, verify knowledge_reuse.tier1_reuse_count >= 1."""

def test_retrospective_counter_idempotency(shared_server):
    """Run retrospective twice on same topic, verify topic_deliveries
    counters are identical after both runs."""
```

**Fixture**: `shared_server` -- state accumulates across test steps within the same module.

**Note**: These tests require seeding observation data, query_log, and injection_log through the MCP interface. If the MCP interface does not expose direct seeding of these tables (it does not -- they are populated by hooks), these integration tests may not be feasible via infra-001. In that case, the Rust-level integration tests in `handler_integration.md` provide equivalent coverage. File a GH Issue for infra-001 enhancement if needed.

### Decision: Integration Test Feasibility

The context_retrospective MCP tool reads from observation tables that are populated by hooks (not by MCP tool calls). The infra-001 harness cannot directly seed observation data, query_log, or injection_log. Therefore:

- **Existing infra-001 tests** validate that retrospective does not regress (existing fields intact, no errors).
- **New MCP-visible behavior** (new optional fields populated) is validated via Rust-level integration tests that have direct Store access.
- **No new infra-001 tests needed** for col-020 specifically. The smoke + tools + lifecycle suites provide regression coverage.
