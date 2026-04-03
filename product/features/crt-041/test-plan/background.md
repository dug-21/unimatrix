# Test Plan: `background` Component

**Source file:** `crates/unimatrix-server/src/background.rs`
**Risk coverage:** R-08, R-13, R-15
**AC coverage:** AC-13, AC-26, AC-27, AC-28, AC-29, AC-30, AC-32

---

## Component Scope

`background.rs` is modified to:
1. Import `run_graph_enrichment_tick` from `services::graph_enrichment_tick`.
2. Call `run_graph_enrichment_tick(store, config, current_tick)` in `run_single_tick` AFTER
   `run_graph_inference_tick`.
3. Update the tick-ordering invariant comment to include `graph_enrichment_tick`.

The S8 gate (`current_tick % config.s8_batch_interval_ticks == 0`) lives inside
`run_graph_enrichment_tick` or is passed down to `run_s8_tick` — the exact placement
must match the pseudocode specification. AC-13 verifies the gate fires correctly.

This component contains NO new SQL logic. Its testing surface is:
- Call ordering (S1/S2/S8 run after graph-rebuild, as specified).
- The S8 gate: `current_tick % s8_batch_interval_ticks == 0`.
- The tick-ordering invariant comment update (AC-27 — grep-verified).
- Pre-flight gate for `write_graph_edge` (AC-28 — grep-verified).

---

## Unit Test Expectations

### Tick Ordering Tests (AC-26)

**`test_run_single_tick_calls_graph_enrichment_after_inference`** — AC-26
- This is a code-review-level assertion, not a pure runtime test. The test cannot directly
  instrument call ordering in an async context without significant mock infrastructure.
- Approach: run `run_single_tick` with a real test DB and observe side effects:
  - After `run_single_tick`, assert S1 edges are present in `graph_edges` for a qualifying
    corpus (confirming `run_graph_enrichment_tick` was called).
  - The ordering guarantee (after `run_graph_inference_tick`) is verified by code review
    of `background.rs`. Add a comment in the test noting the ordering constraint.

**`test_s8_gate_fires_at_correct_tick_multiples`** — AC-13
- Arrange: qualifying S8 data in audit_log. `s8_batch_interval_ticks = 5`.
- Act: call `run_single_tick` with `current_tick = 1` (not a batch tick).
- Assert: zero S8 edges in `graph_edges` with `source='S8'`.
- Act: call `run_single_tick` with `current_tick = 5` (batch tick: 5 % 5 == 0).
- Assert: S8 edges written.

**`test_s8_gate_fires_at_tick_zero`** — AC-13
- Act: `run_single_tick` with `current_tick = 0` and `s8_batch_interval_ticks = 10`.
- Assert: 0 % 10 == 0, so S8 runs. Edges written.

**`test_s8_gate_skips_intermediate_ticks`** — AC-13
- Act: `run_single_tick` with ticks 1, 2, 3, 4 sequentially.
- Assert: S8 edges count unchanged after each (gate fires only at multiples of interval).

### Infallible Tick Tests

**`test_run_single_tick_does_not_panic_with_empty_db`**
- Act: `run_single_tick` with empty DB and default config.
- Assert: no panic. S1, S2, S8 all produce zero edges but do not error.

**`test_run_single_tick_does_not_panic_with_s2_vocabulary_empty`**
- Act: `run_single_tick` with `config.s2_vocabulary = vec![]`.
- Assert: no panic. S2 is a no-op.

---

## Code-Review / Shell Verification Tests (AC-27, AC-28)

These are not runtime `#[test]` items but delivery gate checks run by the tester agent
via shell command. They verify structural code properties.

**AC-27 — Tick-ordering comment updated:**
```bash
grep -n "graph_enrichment_tick" \
    /workspaces/unimatrix/crates/unimatrix-server/src/background.rs
```
Expected: returns at least one match in the tick-ordering invariant comment block AND
one match in the call site itself.

**AC-28 — write_graph_edge prerequisite gate:**
```bash
grep -n "pub(crate) async fn write_graph_edge" \
    /workspaces/unimatrix/crates/unimatrix-server/src/services/nli_detection.rs
```
Expected: returns a match. If absent, this is the hard gate that blocks all S1/S2/S8 call sites.

**AC-31 — File size check:**
```bash
wc -l /workspaces/unimatrix/crates/unimatrix-server/src/services/graph_enrichment_tick.rs
```
Expected: ≤ 500 lines (excluding any sibling `_tests.rs` file).

---

## Integration Test Expectations (MCP Interface)

### GraphCohesionMetrics After S1/S2/S8 (R-13, R-15, AC-29, AC-30, AC-32)

The following integration tests belong in `product/test/infra-001/suites/test_lifecycle.py`.
They verify background.rs-level effects through the MCP interface.

**`test_s1_edges_visible_in_status_after_tick`** — AC-26, AC-32, R-07
```python
@pytest.mark.xfail(
    reason="Background tick interval (15 min default) exceeds integration test timeout. "
    "Test validates MCP-visible S1 edge count increase after tick. "
    "Remove xfail when CI configures short tick interval."
)
def test_s1_edges_visible_in_status_after_tick(shared_server):
    """crt-041 AC-26: S1 edges appear in context_status after one complete tick.

    Pattern: store two entries sharing tagged content, record baseline
    cross_category_edge_count, wait for tick, assert count increased.
    Uses shared_server because state accumulates.
    """
    # Record baseline
    baseline = parse_status_report(shared_server.context_status(agent_id="human", format="json"))
    baseline_cross = baseline.get("cross_category_edge_count", 0)

    # Store two entries sharing tags, different categories (for cross-category edges)
    shared_server.context_store(
        "crt041 s1 test entry decision schema migration performance",
        "crt-041-test",
        "decision",
        agent_id="human",
    )
    shared_server.context_store(
        "crt041 s1 test entry lesson schema migration performance async",
        "crt-041-test",
        "lesson-learned",
        agent_id="human",
    )

    # Wait for tick (polling with 30s timeout)
    import time as _time
    deadline = _time.time() + 30.0
    found = False
    while _time.time() < deadline:
        _time.sleep(2.0)
        report = parse_status_report(
            shared_server.context_status(agent_id="human", format="json")
        )
        if report.get("cross_category_edge_count", 0) > baseline_cross:
            found = True
            break

    assert found, (
        f"crt-041: cross_category_edge_count must increase above baseline {baseline_cross} "
        "after one complete tick with qualifying S1 pairs. AC-26."
    )
```

**`test_inferred_edge_count_unchanged_by_s1_s2_s8`** — R-13, AC-30
```python
@pytest.mark.xfail(
    reason="Background tick interval exceeds test timeout. "
    "Validates inferred_edge_count backward compat after S1/S2/S8 tick."
)
def test_inferred_edge_count_unchanged_by_s1_s2_s8(shared_server):
    """crt-041 AC-30/R-13: inferred_edge_count counts only source='nli' after S1/S2/S8 run.

    1. Record baseline inferred_edge_count and cross_category_edge_count.
    2. Store entries qualifying for S1.
    3. Wait for tick where S1 runs.
    4. Assert inferred_edge_count unchanged (S1 edges are NOT nli-sourced).
    5. Assert cross_category_edge_count increased (S1 wrote edges).
    """
    server = shared_server
    resp0 = server.context_status(agent_id="human", format="json")
    report0 = parse_status_report(resp0)
    baseline_inferred = report0.get("inferred_edge_count", 0)
    baseline_cross = report0.get("cross_category_edge_count", 0)

    # Store entries sharing tags across categories
    server.context_store(
        "crt041 inferred count test schema decision entry unique x7y8z9",
        "crt-041-test",
        "decision",
        agent_id="human",
    )
    server.context_store(
        "crt041 inferred count test schema lesson entry unique x7y8z9",
        "crt-041-test",
        "lesson-learned",
        agent_id="human",
    )

    import time as _time
    deadline = _time.time() + 30.0
    tick_seen = False
    while _time.time() < deadline:
        _time.sleep(2.0)
        resp = server.context_status(agent_id="human", format="json")
        report = parse_status_report(resp)
        if report.get("cross_category_edge_count", 0) > baseline_cross:
            tick_seen = True
            # Backward compat: inferred_edge_count must NOT have changed
            assert report.get("inferred_edge_count", 0) == baseline_inferred, (
                "crt-041 R-13: inferred_edge_count must not count S1/S2/S8 edges. "
                f"Baseline={baseline_inferred}, after tick={report.get('inferred_edge_count', 0)}."
            )
            break

    assert tick_seen, (
        f"crt-041 AC-30: cross_category_edge_count must increase above {baseline_cross}. "
        "If this fails due to tick not firing, confirm xfail reason is accurate."
    )
```

**`test_quarantine_excludes_endpoint_from_graph_traversal`** — R-01, AC-03
```python
def test_quarantine_excludes_endpoint_from_graph_traversal(admin_server):
    """crt-041 AC-03/R-01: quarantined entry excluded from S1 edge generation.

    This test verifies the quarantine guard effect through the MCP interface.
    We cannot directly inspect graph_edges through MCP, but we can verify
    that quarantining an entry causes it to be excluded from search results
    (the existing quarantine exclusion mechanism) — the same status=3 guard
    that the S1/S2/S8 SQL JOINs rely on.

    This is indirect coverage: if status=3 entries are excluded from the
    JOINs that generate edges AND from search results, a test of quarantine
    search exclusion validates the underlying status filter.
    """
    # Store two entries
    resp_a = admin_server.context_store(
        "crt041 quarantine edge test entry alpha schema migration",
        "crt-041-test",
        "decision",
        agent_id="human",
        format="json",
    )
    entry_a_id = extract_entry_id(resp_a)

    resp_b = admin_server.context_store(
        "crt041 quarantine edge test entry beta schema migration",
        "crt-041-test",
        "lesson-learned",
        agent_id="human",
        format="json",
    )
    entry_b_id = extract_entry_id(resp_b)

    # Quarantine entry B
    quarantine_resp = admin_server.context_quarantine(entry_b_id, agent_id="human")
    assert_tool_success(quarantine_resp)

    # Quarantined entry must not appear in search
    search_resp = admin_server.context_search(
        "crt041 quarantine edge test schema migration",
        format="json",
        agent_id="human",
    )
    assert_tool_success(search_resp)
    assert_search_not_contains(search_resp, entry_b_id), (
        "crt-041 R-01: quarantined entry_b must not appear in search results. "
        "The same status=3 guard is used in S1/S2/S8 SQL JOINs."
    )
    # Active entry must still appear
    assert_search_contains(search_resp, entry_a_id), (
        "crt-041: active entry_a must still appear in search after entry_b quarantine."
    )
```

Note: `test_quarantine_excludes_endpoint_from_graph_traversal` does NOT need xfail because
it does not depend on the background tick. The quarantine search-exclusion behavior is
deterministic and immediate.

---

## Tick Ordering Constraint (Architecture Invariant)

The tick ordering after crt-041 must be:
```
... → run_graph_inference_tick → run_graph_enrichment_tick (S1 → S2 → S8 conditional)
```

S1/S2/S8 must run AFTER `TypedGraphState::rebuild` has already run in the same tick.
New edges from this tick are visible to PPR at the NEXT tick's rebuild. This is the
same one-tick delay accepted by co_access_promotion_tick.

**Anti-pattern to guard against:** Do NOT move S1/S2/S8 BEFORE `TypedGraphState::rebuild`.
Doing so would break the established tick-ordering invariant and produce undefined
behavior in PPR traversal (stale graph state mixed with new edges in the same cycle).

---

## Assertions Checklist

- [ ] `run_graph_enrichment_tick` is called after `run_graph_inference_tick` in background.rs — AC-26
- [ ] S8 gate `current_tick % s8_batch_interval_ticks == 0` is correct — AC-13
- [ ] S8 gate correctly fires at tick=0 — AC-13
- [ ] Tick-ordering invariant comment includes `graph_enrichment_tick` — AC-27
- [ ] `write_graph_edge` pre-flight gate passes (grep check) — AC-28
- [ ] `graph_enrichment_tick.rs` ≤ 500 lines (wc -l check) — AC-31 / R-16
- [ ] Integration: `cross_category_edge_count` increases after tick — AC-32
- [ ] Integration: `inferred_edge_count` unchanged after S1/S2/S8 tick — AC-30, R-13
- [ ] Integration: quarantined entry excluded from search (status guard validation) — R-01, AC-03
