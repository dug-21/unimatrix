# Component 5: Integration Tests — `test_tools.py`

## Purpose

Deliver end-to-end integration test coverage for the `DependencyOnDeprecatedRule` detection
path, closing AC-12 from vnc-015. Two test functions: a positive path (finding must appear)
and a negative path (finding must not appear when conditions are absent). Together they
verify the full wiring from MCP tool calls through SQL through the detection pipeline to
the `context_cycle_review` JSON response.

## File

`product/test/infra-001/suites/test_tools.py`

Append after the last test in the vnc-015 section. The vnc-015 section ends at line 3620
(end of file). Add a new section header block followed by the two test functions.

## Imports Required

The following imports are already present at the top of `test_tools.py`:
- `import json`
- `import uuid`
- `import sqlite3`
- `import time`
- `import os`
- Helper functions: `assert_tool_success`, `extract_entry_id`, `get_result_text`,
  `assert_tool_error`
- `_seed_observation_sql` (defined at line 967)
- `_resolve_db_path` (defined at line 955 — resolves `db_path` from `server.project_dir`)

No new imports are needed.

## Section Header

```python
# === vnc-016: DependencyOnDeprecated end-to-end detection test ===========
#
# AC-01, AC-02, AC-03, AC-07, AC-08, AC-09 (Rust layer), AC-12
# R-01, R-02, R-03, R-04, R-06, R-07, R-08
```

## Test 1: Positive Path

### Function Signature

```python
def test_dependency_on_deprecated_e2e(server):
```

The `server` fixture is the process-scoped live MCP server (existing fixture, not modified).

### Step-by-Step Pseudocode

```python
def test_dependency_on_deprecated_e2e(server):
    # -- Step 1: Generate unique cycle ID -----------------------------------
    # Bound once at the top. Used for ALL setup and assertion calls without
    # substitution. A single binding is required (C-03, R-07).
    cycle_id = f"vnc016-{uuid.uuid4().hex[:8]}"

    # -- Step 2: Generate unique test agent ID ------------------------------
    # Unique per invocation prevents cross-test agent-state interference.
    test_agent_id = f"vnc016-agent-{uuid.uuid4().hex[:8]}"

    # -- Step 3: Enroll Restricted+Write agent ------------------------------
    # Admin operation. 'human' has Admin capability by bootstrap.
    # This agent is the realistic production case: Restricted trust + explicit Write.
    # The old gate would have silently dropped feature_entries for this agent.
    # The fixed gate allows it through because require_cap(Write) will pass.
    resp_enroll = server.context_enroll(
        test_agent_id,
        trust_level="restricted",
        capabilities=["write", "read"],
        agent_id="human",
    )
    assert_tool_success(resp_enroll)

    # -- Step 4: Store entry A with feature_cycle, using the Restricted+Write agent --
    # CRITICAL (C-01): feature_cycle MUST be passed here. record_feature_entries is
    # called only at context_store time; there is no back-fill path.
    # CRITICAL (C-01b): MUST use test_agent_id (Restricted+Write), NOT "human"
    # (Privileged). Using "human" exercises a path that always worked; this test
    # must exercise the path that was broken by the trust-level gate.
    resp_a = server.context_store(
        "Deprecated knowledge entry A for vnc-016 e2e test",
        topic=f"vnc016-test-{cycle_id}",
        category="pattern",
        feature_cycle=cycle_id,     # Requires harness client extension (Component 3)
        agent_id=test_agent_id,     # Restricted+Write agent — exercises the fixed gate
    )
    assert_tool_success(resp_a)
    id_a = extract_entry_id(resp_a)

    # -- Step 5: Store entry B without feature_cycle ------------------------
    # B is the target of the Prerequisite edge. It does not need to be in
    # feature_entries. Using "human" or any write-capable agent is acceptable here.
    resp_b = server.context_store(
        "Active knowledge entry B for vnc-016 e2e test",
        topic=f"vnc016-test-{cycle_id}",
        category="pattern",
        agent_id="human",
    )
    assert_tool_success(resp_b)
    id_b = extract_entry_id(resp_b)

    # -- Step 6: Add Prerequisite edge A -> B --------------------------------
    # graph_edges: (source_id=id_a, target_id=id_b, relation_type='Prerequisite')
    # relation_type MUST be exactly 'Prerequisite' (case-sensitive, matches SQL literal).
    resp_edge = server.context_edge("add", id_a, "Prerequisite", id_b, agent_id="human")
    assert_tool_success(resp_edge)

    # -- Step 7: Deprecate entry A via context_correct ----------------------
    # This sets entries.status = 1 for id_a, making the Prerequisite edge stale.
    # context_correct does NOT need feature_cycle (C-06): the SQL query joins on
    # source_id (A), not the successor entry ID.
    resp_correct = server.context_correct(
        id_a,
        "Superseded version of entry A — deprecated for vnc-016 e2e test",
        agent_id="human",
    )
    assert_tool_success(resp_correct)

    # -- Step 8: Seed observation data for cycle_id --------------------------
    # context_cycle_review requires observation rows to produce a detection report.
    # Without them, it takes the "empty feature cycle" early-exit path and returns
    # an acknowledgment (not a RetrospectiveReport with hotspots).
    # MUST use the identical cycle_id bound in Step 1 (C-03).
    # num_records=20 is the default; sufficient (C-04).
    db_path = _resolve_db_path(server.project_dir)
    _seed_observation_sql(db_path, [cycle_id], num_records=20)

    # -- Step 9: Call context_cycle_review with force=True ------------------
    # force=True is MANDATORY (C-02, AC-07). Without it, a cached result from a
    # prior run may be returned, bypassing the detection pipeline entirely (R-02).
    # format="json" required for structured assertion.
    # timeout=30.0 allows time for the detection pipeline.
    resp = server.context_cycle_review(
        cycle_id,
        agent_id="human",
        format="json",
        force=True,     # force=True bypasses memoization — omitting causes vacuous pass on cached result
        timeout=30.0,
    )

    # -- Assertions ----------------------------------------------------------
    # (a) Response must be a successful tool call result (not an MCP error).
    assert_tool_success(resp)

    # (b) Response text must parse as valid JSON.
    result_text = get_result_text(resp)
    data = json.loads(result_text)

    # (c) Top-level 'hotspots' key must be present (its absence means the response
    # is the empty-cycle acknowledgment path, not a RetrospectiveReport — check
    # that _seed_observation_sql received the correct cycle_id if this fails).
    assert "hotspots" in data, (
        f"'hotspots' key absent from response — likely empty-cycle path. "
        f"Response was: {result_text[:500]}"
    )

    # (d) At least one hotspot must have rule_name == "dependency_on_deprecated".
    # Exact string from DependencyOnDeprecatedRule::name() in scope.rs:286.
    rule_names = [h["rule_name"] for h in data["hotspots"]]
    assert any(rn == "dependency_on_deprecated" for rn in rule_names), (
        f"'dependency_on_deprecated' not found in hotspots. "
        f"rule_names present: {rule_names}. "
        f"This test fails if: (1) SQL fix not applied, (2) usage gate not fixed, "
        f"(3) feature_cycle omitted at store time, or (4) wrong agent used at store time."
    )
```

### Why Each Step Is Non-Negotiable

- Step 3 (enroll): Without enrollment, `test_agent_id` resolves to an unenrolled string,
  which may be rejected or resolve to Restricted with no Write capability.
- Step 4 (feature_cycle + test_agent_id): The entire point of this test. Omitting
  `feature_cycle` leaves `feature_entries` empty. Using `"human"` instead of `test_agent_id`
  exercises a path that always worked (Privileged trust always passed the old gate).
- Step 8 (seed): Without 20+ observation rows, `context_cycle_review` returns an
  acknowledgment string, not a JSON object with `hotspots`.
- Step 9 (force=True): Without `force=True`, a cached result from a prior test invocation
  may be returned. Unique `cycle_id` mitigates this but does not eliminate it if the server
  process has cached results from a prior test that happened to use the same cycle_id.

## Test 2: Negative Path

### Function Signature

```python
def test_dependency_on_deprecated_no_finding_without_stale_edge(server):
```

### Purpose

Confirm that `"dependency_on_deprecated"` does NOT appear in hotspots when no stale
Prerequisite edge exists for the cycle. Guards against an "always fires" implementation
(R-04, R-08).

### Step-by-Step Pseudocode

```python
def test_dependency_on_deprecated_no_finding_without_stale_edge(server):
    # -- Step 1: Generate unique cycle ID -----------------------------------
    # Negative path uses distinct prefix "vnc016neg-" to distinguish from positive.
    # Independent cycle ID prevents interference (C-05, NFR-05).
    cycle_id = f"vnc016neg-{uuid.uuid4().hex[:8]}"

    # -- Step 2: Store two entries WITHOUT stale conditions ------------------
    # Neither entry is deprecated. No Prerequisite edge is added between them.
    # agent_id="human" is acceptable here — the purpose is to confirm no false
    # finding, not to validate the gate fix (that is the positive test's job).
    resp_c = server.context_store(
        "Active entry C for vnc-016 negative test",
        topic=f"vnc016neg-test-{cycle_id}",
        category="pattern",
        agent_id="human",
    )
    assert_tool_success(resp_c)

    resp_d = server.context_store(
        "Active entry D for vnc-016 negative test",
        topic=f"vnc016neg-test-{cycle_id}",
        category="pattern",
        agent_id="human",
    )
    assert_tool_success(resp_d)

    # Note: No context_edge call. No context_correct call.
    # The scenario has no stale Prerequisite edge for this cycle.

    # -- Step 3: Seed observation data for cycle_id -------------------------
    # Same requirement as positive test: need observations or the review returns
    # the empty-cycle acknowledgment path, not a detection report.
    db_path = _resolve_db_path(server.project_dir)
    _seed_observation_sql(db_path, [cycle_id], num_records=20)

    # -- Step 4: Call context_cycle_review with force=True ------------------
    # force=True is MANDATORY (C-02). Same requirement as positive test.
    resp = server.context_cycle_review(
        cycle_id,
        agent_id="human",
        format="json",
        force=True,     # force=True bypasses memoization — omitting causes vacuous pass on cached result
        timeout=30.0,
    )

    # -- Assertions ----------------------------------------------------------
    # (a) Response must be successful.
    assert_tool_success(resp)

    # (b) Parse JSON.
    result_text = get_result_text(resp)
    data = json.loads(result_text)

    # (c) hotspots key must be present (confirms detection pipeline ran, not early-exit).
    assert "hotspots" in data, (
        f"'hotspots' key absent from negative-path response. "
        f"Response was: {result_text[:500]}"
    )

    # (d) MUST NOT contain 'dependency_on_deprecated'.
    # Assert on rule_name specifically, NOT on hotspots being empty (R-08):
    # other rules may legitimately fire; asserting total absence would produce
    # false test failures when other rules fire on these entries.
    rule_names = [h["rule_name"] for h in data["hotspots"]]
    assert not any(rn == "dependency_on_deprecated" for rn in rule_names), (
        f"'dependency_on_deprecated' unexpectedly present in hotspots. "
        f"rule_names: {rule_names}. "
        f"This indicates an always-fires implementation or cross-test cycle contamination."
    )
```

### Assertion Design (R-08)

`assert not any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"])`
not `assert data["hotspots"] == []`.

The second form would fail if ANY other detection rule fires on the seeded entries (which is
valid and expected for other rules). The first form specifically checks only the rule under
test, allowing other rules to appear without failing the test.

## Error Handling (Python Test Level)

- `assert_tool_success(resp)` fails with a legible message if the MCP server returns an error
  response. If Step 9 fails here, likely causes: capability error, server crash, timeout.
- `json.loads(result_text)` raises `json.JSONDecodeError` if the response is not valid JSON.
  This would indicate the server returned the empty-cycle acknowledgment string instead of a
  JSON report — root cause is likely `_seed_observation_sql` not called with matching cycle_id.
- `assert "hotspots" in data` fails with `AssertionError` if the JSON lacks the key. The
  error message includes the response text for diagnosis.
- All assertion messages include enough diagnostic context to identify the root cause without
  reading the server logs.

## Key Test Scenarios

| Scenario | Expected Result | Root Cause if Wrong |
|----------|----------------|---------------------|
| SQL fix applied, gate fixed, correct agent, cycle_id consistent | Positive: finding present | — |
| SQL fix NOT applied | Positive: finding absent (test fails) | Column error swallowed by tools.rs unwrap_or_else |
| Usage gate NOT fixed | Positive: finding absent (test fails) | feature_entries not written for Restricted agent |
| `feature_cycle` omitted at store time | Positive: finding absent (test fails) | feature_entries row not created |
| Wrong agent_id (e.g., "human") at store time | Positive: appears to pass (but vacuous) | Old gate allowed Privileged — test provides no regression signal |
| `force=True` omitted | Either: may return stale cached result | Memoization bypass skipped |
| cycle_id mismatch between store and seed | Positive: finding absent | Empty-cycle early-exit path taken |
| No observation rows seeded | Positive: JSON parse fails or hotspots key absent | Empty-cycle path; not a detection report |
| No stale edge (negative test) | Negative: no finding | Correct behavior |
| Always-fires rule bug | Negative: finding present (test fails) | DependencyOnDeprecatedRule fires without stale edge |

## Constraints

- C-01: `feature_cycle=cycle_id` on the `context_store` call for entry A — mandatory.
- C-01b: `agent_id=test_agent_id` (Restricted+Write) on the `context_store` call for entry A.
- C-02: `force=True` on every `context_cycle_review` call in both tests.
- C-03: Single `cycle_id` binding per test function. Used consistently.
- C-04: `num_records=20` (default). Never zero.
- C-05: Each test generates its own `cycle_id` with unique prefix.
- C-07: No new test files.
- C-09: `uds_client.py` not modified.
- NFR-05: Tests are fully self-contained — unique cycle IDs, unique agent IDs, own entry setup.
