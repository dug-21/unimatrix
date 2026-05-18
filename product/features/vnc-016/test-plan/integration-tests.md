# Component Test Plan: Integration Tests (`test_tools.py`)

## Component

**File**: `product/test/infra-001/suites/test_tools.py`, vnc-015 section (line 3048+)
**New functions**:
- `test_dependency_on_deprecated_e2e`
- `test_dependency_on_deprecated_no_finding_without_stale_edge`

Both use the `server` fixture (function scope — fresh DB, no state leakage).
No new test file. No new fixture.

---

## AC Coverage

| AC-ID | Description |
|-------|-------------|
| AC-01 | `test_dependency_on_deprecated_e2e` exists and passes |
| AC-02 | Test executes the 9-step scenario in stated order |
| AC-03 | Assertion checks `rule_name == "dependency_on_deprecated"` in `hotspots` |
| AC-07 | Both tests call `context_cycle_review` with `force=True` |
| AC-08 | `test_dependency_on_deprecated_no_finding_without_stale_edge` passes |

## Risk Coverage

| Risk ID | How These Tests Address It |
|---------|---------------------------|
| R-01 | Positive test fails against un-fixed code (pre-fix regression verification required by implementer) |
| R-02 | `force=True` literal in both `context_cycle_review` calls; unique `cycle_id` per function |
| R-03 | Indirectly covered — integration test fails if SQL query returns empty |
| R-04 | Negative companion confirms the rule does not always fire |
| R-06 | Step 4 uses `agent_id=test_agent_id` (Restricted+Write), NOT `"human"` |
| R-07 | Single `cycle_id` binding at top of each test; same variable everywhere |
| R-08 | Assertion uses `not any(h["rule_name"] == "dependency_on_deprecated" ...)`, not `hotspots == []` |

---

## Test 1: `test_dependency_on_deprecated_e2e`

### Fixture

`server` — function scope. Fresh SQLite database. No state from prior tests.

### Step-by-Step

```python
def test_dependency_on_deprecated_e2e(server):
    """AC-01, AC-02, AC-03, AC-07, AC-12.

    Positive path: verify DependencyOnDeprecatedRule fires end-to-end when a Restricted+Write
    agent stores entry A tagged to a feature cycle, a Prerequisite edge exists A→B, and A is
    subsequently deprecated.

    force=True is mandatory — omitting it causes the handler to return a cached result,
    bypassing the detection pipeline and making this test vacuously pass.
    """
    import json, uuid, time

    # ---- Step 1: unique cycle ID, bound once at the top ----
    cycle_id = f"vnc016-{uuid.uuid4().hex[:8]}"

    # ---- Step 2: unique agent ID ----
    test_agent_id = f"vnc016-agent-{uuid.uuid4().hex[:8]}"

    # ---- Step 3: enroll Restricted+Write agent ----
    # human has Admin capability (bootstrap default); required for context_enroll
    enroll_resp = server.context_enroll(
        test_agent_id,
        trust_level="restricted",
        capabilities=["write", "read"],
        agent_id="human",
    )
    assert_tool_success(enroll_resp)

    # ---- Step 4: store entry A with feature_cycle, using the enrolled agent ----
    # CRITICAL (C-01, C-01b): must pass feature_cycle AND must use test_agent_id (Restricted).
    # Using "human" (Privileged) exercises the path that always passed the old gate.
    # Using test_agent_id exercises the fixed path (write_capable=True replaces old trust gate).
    resp_a = server.context_store(
        "vnc016 prerequisite source: ADR establishing the indexing strategy — now deprecated",
        "architecture",
        "decision",
        feature_cycle=cycle_id,
        agent_id=test_agent_id,
        format="json",
    )
    assert_tool_success(resp_a)
    id_a = extract_entry_id(resp_a)

    # ---- Step 5: store entry B (the target of the Prerequisite edge) ----
    resp_b = server.context_store(
        "vnc016 prerequisite target: operational runbook that depends on the deprecated ADR",
        "operations",
        "convention",
        agent_id="human",
        format="json",
    )
    assert_tool_success(resp_b)
    id_b = extract_entry_id(resp_b)

    # ---- Step 6: add Prerequisite edge A→B ----
    edge_resp = server.context_edge("add", id_a, "Prerequisite", id_b, agent_id="human")
    assert_tool_success(edge_resp)

    # ---- Step 7: deprecate entry A via context_correct ----
    # context_correct does not need feature_cycle (C-06).
    # The SQL query joins on source entry A's membership in feature_entries,
    # which was established in step 4.
    correct_resp = server.context_correct(
        id_a,
        "vnc016 corrected: updated ADR replacing the deprecated indexing strategy",
        agent_id="human",
    )
    assert_tool_success(correct_resp)

    # ---- Step 8: seed observation data for cycle_id ----
    db_path = _resolve_db_path(server.project_dir)
    _seed_observation_sql(db_path, [cycle_id], num_records=20)

    # ---- Step 9: call context_cycle_review with force=True ----
    # force=True is mandatory (C-02). Omitting it risks hitting a cached result.
    resp = server.context_cycle_review(
        cycle_id,
        agent_id="human",
        format="json",
        force=True,
        timeout=30.0,
    )
    assert_tool_success(resp)

    # ---- Assertions ----
    result_text = get_result_text(resp)
    data = json.loads(result_text)

    # hotspots key must be present (not early-exit acknowledgment path)
    assert "hotspots" in data, (
        f"'hotspots' key missing from context_cycle_review response. "
        f"Response may be early-exit path (no observations for cycle). "
        f"Keys: {list(data.keys())}"
    )

    hotspots = data["hotspots"]
    rule_names = [h["rule_name"] for h in hotspots]
    assert any(h["rule_name"] == "dependency_on_deprecated" for h in hotspots), (
        f"Expected 'dependency_on_deprecated' in rule_names, got: {rule_names}. "
        f"If this list is empty, check that feature_entries was written (C-01, C-01b) "
        f"and that the SQL fix in read.rs is applied (AC-04)."
    )
```

### Step Order Dependency

Steps must execute in order: enroll → store A → store B → edge → deprecate → seed → review.
- Steps 3 must precede step 4 (agent must exist before first use).
- Step 4 must precede step 6 (entry A must exist before edge can reference it).
- Step 7 must precede step 9 (A must be deprecated before review).
- Step 8 must precede step 9 (observations must exist for review to produce a detection report).

---

## Test 2: `test_dependency_on_deprecated_no_finding_without_stale_edge`

### Fixture

`server` — same function scope, independent DB state.

### Implementation

```python
def test_dependency_on_deprecated_no_finding_without_stale_edge(server):
    """AC-08, R-04, R-08.

    Negative path: verify DependencyOnDeprecatedRule does NOT fire when no stale Prerequisite
    edge exists for the cycle. Guards against an always-fires implementation.

    force=True is mandatory — same reason as positive test.
    Assertion uses rule_name check, not total hotspot absence (R-08).
    """
    import json, uuid

    # Independent cycle ID — never reused from the positive test (C-05)
    cycle_id = f"vnc016neg-{uuid.uuid4().hex[:8]}"

    # Store two entries; no Prerequisite edge; neither is deprecated
    resp_c = server.context_store(
        "vnc016 negative test entry C: active convention with no stale edge",
        "architecture",
        "convention",
        agent_id="human",
        format="json",
    )
    assert_tool_success(resp_c)

    resp_d = server.context_store(
        "vnc016 negative test entry D: active convention target with no edge pointing to it",
        "operations",
        "convention",
        agent_id="human",
        format="json",
    )
    assert_tool_success(resp_d)

    # Seed observations for the same cycle_id
    db_path = _resolve_db_path(server.project_dir)
    _seed_observation_sql(db_path, [cycle_id], num_records=20)

    # Call context_cycle_review — force=True mandatory (C-02)
    resp = server.context_cycle_review(
        cycle_id,
        agent_id="human",
        format="json",
        force=True,
        timeout=30.0,
    )
    assert_tool_success(resp)

    result_text = get_result_text(resp)
    data = json.loads(result_text)

    assert "hotspots" in data, (
        f"'hotspots' key missing from context_cycle_review response. "
        f"Keys: {list(data.keys())}"
    )

    # Rule-name specific assertion (R-08): do NOT assert hotspots == []
    # Other rules may legitimately fire; we only care that dependency_on_deprecated does not.
    assert not any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"]), (
        f"Expected 'dependency_on_deprecated' to be absent when no stale edge exists. "
        f"Found rule in hotspots: {[h['rule_name'] for h in data['hotspots']]}"
    )
```

---

## Hard Constraints in Both Tests

| Constraint | Requirement | Failure Mode If Violated |
|-----------|-------------|--------------------------|
| C-01 | `feature_cycle=cycle_id` passed to `context_store` for entry A | `feature_entries` not populated; SQL returns empty; false negative |
| C-01b | `agent_id=test_agent_id` (Restricted+Write) at step 4 | Old trust gate passes; test vacuously passes before fix |
| C-02 | `force=True` on every `context_cycle_review` call | Cached result returned; detection pipeline bypassed; vacuous pass |
| C-03 | Same `cycle_id` binding used everywhere in each test | Empty-cycle early-exit; `hotspots` key absent; `KeyError` |
| C-04 | `num_records=20` (default) in `_seed_observation_sql` | Empty-cycle path triggered; no detection report |
| C-05 | Unique `cycle_id` per test function | Cross-test interference from shared live-server process |
| C-07 | No new test files | Append to `test_tools.py` in vnc-015 section only |

---

## Pytest Run Commands

```bash
cd product/test/infra-001

# Run positive test only
python -m pytest suites/test_tools.py::test_dependency_on_deprecated_e2e -v --timeout=60

# Run negative test only
python -m pytest suites/test_tools.py::test_dependency_on_deprecated_no_finding_without_stale_edge -v --timeout=60

# Run both new tests
python -m pytest suites/test_tools.py -k "vnc016 or dependency_on_deprecated" -v --timeout=60

# Run full tools suite (regression)
python -m pytest suites/test_tools.py -v --timeout=60 2>&1 | tail -30
```

---

## Failure Triage Guide

| Failure Symptom | Root Cause | Fix |
|----------------|------------|-----|
| `KeyError: 'hotspots'` | `_seed_observation_sql` not called, wrong `cycle_id`, or `num_records=0` | Verify step 8 uses same `cycle_id` and `num_records=20` |
| `AssertionError: 'dependency_on_deprecated' not in rule_names` | SQL fix not applied, OR `feature_entries` not written (C-01/C-01b gate issue) | Verify `read.rs:1618` fix; verify `usage.rs` gate fix; verify `agent_id=test_agent_id` |
| `AssertionError: expected None when write_capable=false` | Gate logic not updated in `usage.rs` | Verify both gate blocks replaced with `ctx.write_capable` |
| `MCPError: capability required: Write` from `context_enroll` | Enrollment call using wrong `agent_id` caller | Verify enrollment uses `agent_id="human"` (Admin) |
| Positive test passes before SQL fix is applied | Test is vacuous — assertions are wrong or `feature_cycle` was not passed | Verify `feature_cycle=cycle_id` in step 4; verify assertion is `any(...)`, not `len > 0` |

---

## Integration with infra-001 Suite Selection

The two new tests live in `test_tools.py` and use the `server` fixture. They exercise:
- Tool calls (`context_store`, `context_enroll`, `context_edge`, `context_correct`,
  `context_cycle_review`) — `tools` suite coverage
- Lifecycle behavior (`feature_entries` written during store, read during cycle_review) — `lifecycle` suite coverage
- Capability enforcement (`require_cap(Write)` for Restricted+Write agent) — `security` suite coverage

Running `pytest suites/test_tools.py` in Stage 3c covers these tests. The smoke gate
(`-m smoke`) does not include these new tests (they are not marked `smoke`), but the smoke
gate must still pass as a baseline.
