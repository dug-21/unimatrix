# SPECIFICATION: vnc-016 — End-to-End Integration Test for DependencyOnDeprecated Detection Rule

## Objective

vnc-016 closes the wiring gap left by vnc-015 AC-12 (PARTIAL). The feature fixes a confirmed SQLite column-name bug in `query_stale_prerequisite_edges_for_cycle`, adds a Rust unit test that would have caught that bug, extends the Python harness client to expose the `feature_cycle` parameter on `context_store`, and delivers a positive-path plus a negative-path integration test that together verify the full wiring from the MCP layer through SQL through the detection pipeline to the `context_cycle_review` JSON response. The feature also fixes a confirmed production bug in `usage.rs` where the `feature_entries` write gate uses trust level instead of Write capability, silently dropping cycle attribution for Restricted-trust agents that have been explicitly granted Write capability.

---

## Functional Requirements

### FR-01: SQL Fix in `read.rs`

FR-01.1 The string literal `fe.feature_cycle` on line 1618 of `crates/unimatrix-store/src/read.rs` must be replaced with `fe.feature_id`, matching the column name defined in the `feature_entries` table schema (`db.rs:616–621`, confirmed write paths: `write_ext.rs:274`, `analytics.rs:687`).

FR-01.2 After the fix, calling `query_stale_prerequisite_edges_for_cycle(cycle)` against a database that contains a `feature_entries` row `(feature_id = cycle, entry_id = A)`, a `graph_edges` row `(source_id = A, target_id = B, relation_type = 'Prerequisite')`, and an `entries` row where `A.status = 1` (Deprecated) must return `vec![(A, B)]` without error.

FR-01.3 The fix must not change the function signature, return type, or any other SQL clause in the query.

---

### FR-02: Rust Unit Test for `query_stale_prerequisite_edges_for_cycle`

FR-02.1 A new `#[tokio::test]` in `crates/unimatrix-store/src/read.rs` (test module) must exercise `query_stale_prerequisite_edges_for_cycle` directly against the store's in-memory SQLite fixture.

FR-02.2 The test must:

a. Insert two entries into `entries`: entry A with `status = 1` (Deprecated) and entry B with `status = 0` (Active).
b. Insert a row into `feature_entries` with `(feature_id = <cycle>, entry_id = A, phase = NULL)`.
c. Insert a row into `graph_edges` with `(source_id = A, target_id = B, relation_type = 'Prerequisite')`.
d. Call `store.query_stale_prerequisite_edges_for_cycle(<cycle>).await`.
e. Assert the returned `Vec<(u64, u64)>` is non-empty (length == 1) and contains the pair `(A, B)`.

FR-02.3 A companion negative-path variant of the unit test must assert that calling `query_stale_prerequisite_edges_for_cycle` with a cycle ID that has no `feature_entries` rows returns an empty `Vec`.

FR-02.4 No new test infrastructure (fixtures, helpers, feature flags) may be introduced; the test must use the store-layer test harness already established in `read.rs`.

---

### FR-03: Harness Client Extension (`harness/client.py`)

FR-03.1 The `context_store()` method in `product/test/infra-001/harness/client.py` must accept an optional keyword argument `feature_cycle: str | None = None`.

FR-03.2 When `feature_cycle` is not `None`, it must be inserted into the `args` dict under the key `"feature_cycle"` before the `call_tool` call, following the existing guard pattern (`if feature_cycle is not None: args["feature_cycle"] = feature_cycle`).

FR-03.3 When `feature_cycle` is `None` (the default), the `args` dict must not contain the `"feature_cycle"` key. This preserves full backward compatibility: existing `context_store` call sites that do not pass `feature_cycle` are unaffected.

FR-03.4 The `StoreParams` struct in `tools.rs` declares `feature_cycle: Option<String>` without `#[serde(default)]`. Both an absent key and an explicit JSON `null` deserialize to `Option::None`. The harness client must pass the key only when non-None (FR-03.2), which matches this contract.

FR-03.5 `uds_client.py` must NOT be modified. The `feature_cycle` parameter on `context_store` is a lower-level escape hatch for test setup; the correct production path for session-context association is `context_cycle`, which `uds_client.py` already supports.

---

### FR-04: UsageContext Gate Fix (`usage.rs`)

FR-04-gate.1 The `UsageContext` struct in `crates/unimatrix-server/src/services/usage.rs` must gain a new field `write_capable: bool`. This field has no default; every construction site must explicitly set it. Clippy's exhaustive-struct-construction check enforces this.

FR-04-gate.2 The `feature_recording` eligibility gate in `record_mcp_usage` (lines 207–218) must be changed from a trust-level match to `ctx.write_capable`. The new logic is:

```
let feature_recording = ctx.feature_cycle.and_then(|feature_str| {
    if ctx.write_capable {
        Some((feature_str, entry_ids.to_vec()))
    } else {
        None
    }
});
```

The fields `ctx.trust_level` and `TrustLevel` must no longer appear in this gate.

FR-04-gate.3 The identical gate in `record_hook_injection` (lines 272–283) must be changed in the same way: replace the trust-level match with `if ctx.write_capable`.

FR-04-gate.4 All existing `UsageContext` construction sites outside the `context_store` handler must set `write_capable: false`. This preserves current behavior: no other tool path writes `feature_entries`.

FR-04-gate.5 The `context_store` handler's `UsageContext` construction site (tools.rs line ~826) must set `write_capable: true`. The `require_cap(Capability::Write)` guard at line 653 has already verified the agent's Write capability before this site is reached; `write_capable: true` is therefore unconditional at that site.

FR-04-gate.6 The `trust_level` field on `UsageContext` must be retained if other callers rely on it. Removing it is not in scope. Only the feature-recording gate may no longer use it.

---

### FR-05: Integration Tests (Positive Path)

FR-05.1 A test function named `test_dependency_on_deprecated_e2e` must be added to `product/test/infra-001/suites/test_tools.py` in the vnc-015 section (after line 3048). No new test file may be created.

FR-05.2 The test must execute the following nine-step scenario in order:

1. Generate a unique `cycle_id = f"vnc016-{uuid.uuid4().hex[:8]}"` at the top of the test function. This binding must be used for all setup and assertion steps without substitution.
2. Generate a unique `test_agent_id = f"vnc016-agent-{uuid.uuid4().hex[:8]}"`. This agent will be used for the `context_store` call that tags entry A to the cycle.
3. Enroll the test agent via `server.context_enroll(test_agent_id, trust_level="restricted", capabilities=["write", "read"], agent_id="human")`. The `human` agent has Admin capability and can enroll agents. This agent represents the realistic production case: a Restricted-trust orchestrator agent with explicit Write grant.
4. Store entry A via `server.context_store(..., feature_cycle=cycle_id, agent_id=test_agent_id)`. The Restricted+Write agent exercises the fixed gate path: `write_capable=True` is set at the handler callsite (require_cap already verified), and `feature_entries` is written. This step is non-deferrable and non-reorderable.
5. Store entry B via `server.context_store(...)` without `feature_cycle` (agent_id may be "human" or any write-capable agent).
6. Add a `Prerequisite` edge A→B via `server.context_edge("add", id_a, "Prerequisite", id_b)`.
7. Deprecate entry A via `server.context_correct(id_a, ...)`. This sets `A.status = 1` in `entries`, making the edge stale. The `context_correct` call does not need to pass `feature_cycle`.
8. Seed observation data for `cycle_id` via `_seed_observation_sql(db_path, [cycle_id])` with at least 20 rows (the default `num_records=20` is sufficient). The same `cycle_id` used in step 1 must be passed here.
9. Call `server.context_cycle_review(cycle_id, agent_id="human", format="json", force=True, timeout=30.0)`.

FR-05.3 The test must assert that:

a. The `context_cycle_review` response is successful (`assert_tool_success`).
b. The response text parses as valid JSON.
c. The parsed JSON object contains a top-level key `"hotspots"` whose value is a JSON array.
d. At least one element of `hotspots` has `rule_name == "dependency_on_deprecated"`. The exact string to assert is `"dependency_on_deprecated"` (source: `DependencyOnDeprecatedRule::name()` in `crates/unimatrix-observe/src/detection/scope.rs:286`).

FR-05.4 The JSON assertion path is: `response_json["hotspots"][i]["rule_name"] == "dependency_on_deprecated"` for some index `i`. In Python: `any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"])`.

FR-05.5 The test validates the fixed gate path. If the gate were not fixed, the `context_store` call in step 4 would silently drop the `feature_entries` write (Restricted trust was excluded by the old gate), the SQL query would return `vec![]`, and the assertion in FR-05.3d would fail. Passing this test after the fix confirms the Restricted+Write path is unblocked.

---

### FR-06: Integration Tests (Negative Path)

FR-06.1 A test function named `test_dependency_on_deprecated_no_finding_without_stale_edge` must be added immediately after `test_dependency_on_deprecated_e2e` in the same file and section.

FR-06.2 The test must generate its own independent `cycle_id = f"vnc016neg-{uuid.uuid4().hex[:8]}"` at the top of the function. It must not share the cycle ID, entries, or observation rows from the positive-path test.

FR-06.3 The test must:

1. Store two entries (C and D) without a Prerequisite edge between them, or with no edge at all. Neither entry needs to be deprecated. Using `agent_id="human"` is acceptable here because the purpose of this test is to confirm no stale-edge finding fires — not to validate the gate fix.
2. Seed at least 20 observation rows for the same `cycle_id` via `_seed_observation_sql(db_path, [cycle_id])`.
3. Call `server.context_cycle_review(cycle_id, agent_id="human", format="json", force=True, timeout=30.0)`.

FR-06.4 The test must assert:

a. The response is successful.
b. The `"hotspots"` array does NOT contain any element with `rule_name == "dependency_on_deprecated"`.

---

### FR-07: Regression Test for Gate Logic (Unit Test)

FR-07.1 A new test (or two test functions) in `crates/unimatrix-server/src/services/usage.rs` (test module) must verify the `write_capable` gate in isolation.

FR-07.2 One test must construct a `UsageContext` with `write_capable: false` and `feature_cycle: Some("test-cycle".to_string())` and confirm the gate evaluation yields `None` (no write is enqueued). This is the negative branch.

FR-07.3 A companion test must construct a `UsageContext` with `write_capable: true` and `feature_cycle: Some("test-cycle".to_string())` and confirm the gate evaluation yields `Some(...)`. This is the positive branch.

FR-07.4 These tests are pure unit tests on the gate logic. They do not require a live store, database, or MCP server. They may use the existing test module pattern in `usage.rs`.

---

## Non-Functional Requirements

NFR-01 All existing tests in `product/test/infra-001/suites/test_tools.py` and `test_lifecycle.py` must continue to pass after the changes. The harness client change is additive; no existing call site signature is altered.

NFR-02 The Rust unit test (FR-02) must pass under `cargo test -p unimatrix-store` with zero failures. The SQL fix (FR-01) must also cause any previously failing SQL-level assertions to pass.

NFR-03 No new MCP tool, parameter, or schema change is introduced. The `context_store` MCP tool already accepts `feature_cycle`; only the Python harness client gains a new optional parameter.

NFR-04 No new Python dependencies may be added to the infra-001 test suite.

NFR-05 The integration tests must not rely on test-order side-effects. Each test function must be fully self-contained (unique cycle ID, unique agent ID, own entry setup, own observation seeding).

NFR-06 `cargo fmt` and `cargo clippy --workspace -- -D warnings` must pass after the Rust changes.

NFR-07 The `write_capable` field must have no `#[serde(default)]` or Rust `Default` derivation. Every `UsageContext` construction site must be an explicit decision. This is enforced by Rust's exhaustive struct construction: omitting the field is a compile error.

NFR-08 The `trust_level` field on `UsageContext` must be preserved. The scope of the gate fix is limited to the feature-recording guard only; no other logic that currently reads `trust_level` may be changed by this feature.

---

## Acceptance Criteria

| AC-ID | Statement | Verification Method |
|-------|-----------|---------------------|
| AC-01 | `test_dependency_on_deprecated_e2e` exists in `test_tools.py` and passes against the live MCP server | `pytest product/test/infra-001/suites/test_tools.py -k test_dependency_on_deprecated_e2e` exits 0 |
| AC-02 | The test executes the 9-step scenario (FR-05.2) exactly: enroll Restricted+Write agent, store A with `feature_cycle` and that agent_id, store B, add Prerequisite A→B, deprecate A, seed observations with same cycle ID, call `context_cycle_review(force=True)`, assert finding | Code inspection of test body; all 9 steps present and in stated order |
| AC-03 | The assertion checks `rule_name == "dependency_on_deprecated"` in the `hotspots` array of the JSON response (exact string, from `scope.rs:286`) | Code inspection: `any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"])` |
| AC-04 | `query_stale_prerequisite_edges_for_cycle` in `read.rs` uses `fe.feature_id` in the WHERE clause, not `fe.feature_cycle` | Code inspection of `read.rs:1618`; `cargo test -p unimatrix-store` passes |
| AC-05 | `harness/client.py` `context_store()` accepts and forwards optional `feature_cycle` keyword argument; absent by default; forwarded only when non-None | Code inspection; existing call sites compile and pass without modification |
| AC-06 | All existing integration tests pass without regression | Full pytest run exits 0; `cargo test --workspace` exits 0 |
| AC-07 | Both integration tests call `context_cycle_review` with `force=True` | Code inspection of both test function bodies |
| AC-08 | `test_dependency_on_deprecated_no_finding_without_stale_edge` passes: no `dependency_on_deprecated` hotspot in response when no stale Prerequisite edge exists | `pytest product/test/infra-001/suites/test_tools.py -k test_dependency_on_deprecated_no_finding_without_stale_edge` exits 0 |
| AC-09 | A Rust unit test in `unimatrix-store` calls `query_stale_prerequisite_edges_for_cycle` directly, asserts the returned `Vec<(u64, u64)>` is non-empty and contains the seeded `(A, B)` pair | `cargo test -p unimatrix-store` exits 0; test function body contains `assert!(!result.is_empty())` or equivalent |
| AC-10 | `UsageContext` in `usage.rs` has a `write_capable: bool` field with no `#[serde(default)]` or `Default` derivation | `cargo build --workspace` exits 0; code inspection confirms field presence and no default |
| AC-11 | The `feature_recording` gate in both `record_mcp_usage` and `record_hook_injection` checks `ctx.write_capable` (not trust level) | Code inspection of both gate blocks; `TrustLevel` not referenced in either gate |
| AC-12 | The `context_store` handler's `UsageContext` construction sets `write_capable: true`; all other `UsageContext` construction sites set `write_capable: false` | Code inspection of all `UsageContext { ... }` literals in `tools.rs` and `usage.rs` |
| AC-13 | A unit test in `unimatrix-server` verifies that `write_capable: false` with a non-None `feature_cycle` yields no write, and `write_capable: true` with a non-None `feature_cycle` yields a write | `cargo test -p unimatrix-server` exits 0; test body exercises both branches of the gate |

---

## Domain Models

**entry (A, B)**: A row in the `entries` table. `status = 0` is Active; `status = 1` is Deprecated. The source entry in a stale Prerequisite edge is always Deprecated (status 1).

**feature_entries**: Junction table `(feature_id TEXT, entry_id INTEGER, phase TEXT)`. Records which entries were stored during a given feature cycle. The `feature_id` column is the cycle identifier string (e.g., `"vnc016-a3b4c5d6"`). This column name is authoritative; `feature_cycle` is the name used in application code for the value passed in, not for the column.

**graph_edges**: Directed edges `(source_id, target_id, relation_type)`. A `Prerequisite` edge from A to B declares that A depends on B. When A is Deprecated, the edge is stale.

**stale Prerequisite edge**: A `graph_edges` row where `relation_type = 'Prerequisite'`, `source_id = A`, and the entry with `id = A` has `status = 1`. Detected by `query_stale_prerequisite_edges_for_cycle(cycle)` via the three-way JOIN on `graph_edges`, `entries`, and `feature_entries`.

**feature cycle**: A string identifier (e.g., `"vnc-016"`, `"vnc016-a3b4c5d6"`) that groups entries and observation sessions. The same string must appear in `feature_entries.feature_id` (for entries) and in `sessions.feature_cycle` (for observations) for `context_cycle_review` to associate them.

**DependencyOnDeprecatedRule**: A `DetectionRule` implementation in `unimatrix-observe`. Its `rule_name` is the string `"dependency_on_deprecated"` (returned by `name()` and stored in `HotspotFinding.rule_name`). It is driven entirely by the `stale_edge_pairs` injected at construction time (ADR-004: constructor injection); it does not perform any I/O.

**context_cycle_review**: The 12th MCP tool. When called with `force=True`, it runs the full detection pipeline fresh, bypassing the memoization cache. When no observations exist for the cycle, it returns an early acknowledgment path, not a detection report. The response is a serialized `RetrospectiveReport` struct; `hotspots` is the top-level JSON array field holding `HotspotFinding` objects.

**analytics write path**: When `context_store` is called with a non-empty `feature_cycle`, the handler enqueues a `UsageContext` with that value (and `write_capable: true`) into `UsageService.record_access`. After the gate fix, the service evaluates `ctx.write_capable` and, when true, calls `record_feature_entries(feature_cycle, [entry_id], phase)`, which writes `(feature_id = feature_cycle, entry_id = entry_id)` into `feature_entries` via `INSERT OR IGNORE`. Before the fix, Restricted-trust agents were silently excluded even when `write_capable` would have been true — the old gate checked trust level, not capability. This is the only write path for `feature_entries` in the MCP server; there is no back-fill mechanism.

**write_capable**: A boolean field on `UsageContext`. Set to `true` only at the `context_store` callsite, where `require_cap(Capability::Write)` has already verified the agent's Write grant. Set to `false` at all other `UsageContext` construction sites. Determines whether `feature_entries` is written when `feature_cycle` is present. Decouples the feature-recording gate from trust level, enabling Restricted-trust agents with explicit Write capability to attribute entries to a cycle.

**Restricted+Write agent**: An agent enrolled with `trust_level="restricted"` and `capabilities=["write", "read"]`. Before this fix, calling `context_store` with a `feature_cycle` from such an agent would silently drop the `feature_entries` write. After the fix, the write proceeds because `write_capable: true` is set by the handler (not derived from trust level). This is the canonical production case for orchestrator agents that should be granted write access without full trust escalation.

---

## User Workflows

### Workflow 1: Developer verifying the SQL fix is correct

1. Run `cargo test -p unimatrix-store test_query_stale_prerequisite_edges_for_cycle`.
2. Observe: test passes, returned vec contains the seeded `(A, B)` pair.
3. Before the fix, the test fails with a Database error containing "no such column: fe.feature_cycle".

### Workflow 2: CI running the full integration suite

1. `cargo test --workspace` — Rust unit tests pass including FR-02 test.
2. `pytest product/test/infra-001/suites/test_tools.py` — all tests pass including AC-01 and AC-08.

### Workflow 3: Privileged agent storing an entry tagged to a feature cycle

Via the extended harness client (human or other Privileged agent — always worked):
```python
resp = server.context_store(
    content="...",
    topic="...",
    category="decision",
    feature_cycle="vnc-016",
    agent_id="human",
)
```
This causes `feature_entries` to be populated with `(feature_id="vnc-016", entry_id=<new_id>)` via the analytics write path. A subsequent `context_cycle_review("vnc-016", force=True)` can then detect stale edges on that entry.

### Workflow 4: Restricted+Write orchestrator agent storing an entry tagged to a cycle

After the gate fix, a Restricted-trust agent with explicit Write capability can attribute entries:
```python
# Admin enrolls the orchestrator agent
server.context_enroll(
    "my-orchestrator-001",
    trust_level="restricted",
    capabilities=["write", "read"],
    agent_id="human",
)

# Orchestrator stores entry with cycle attribution
resp = server.context_store(
    content="...",
    topic="...",
    category="convention",
    feature_cycle="col-031",
    agent_id="my-orchestrator-001",
)
```
Before the fix, the `feature_entries` write was silently dropped for `my-orchestrator-001` because its trust level is Restricted. After the fix, `write_capable: true` is set in the handler (require_cap already verified Write), and `feature_entries` is written.

### Workflow 5: Verifying the gate fix with a unit test

```rust
// In usage.rs test module
let ctx = UsageContext {
    feature_cycle: Some("test-cycle".to_string()),
    write_capable: false,
    // ... other fields
};
// Gate evaluation: write_capable=false → None
let gate_result = ctx.feature_cycle.and_then(|f| {
    if ctx.write_capable { Some(f) } else { None }
});
assert!(gate_result.is_none());
```

---

## Constraints

**C-01 (from SR-02, High risk): `feature_cycle` must be passed at `context_store` time for entry A, not deferred.** `record_feature_entries` is called via the analytics write path only during `context_store`. No back-fill path exists. If step 4 of the positive-path test omits `feature_cycle`, the SQL query returns `vec![]` silently and the test becomes a false negative. This is the highest-priority constraint for the implementer.

**C-01b (gate fix constraint): The `context_store` call for entry A in the positive-path test MUST use a Restricted+Write agent, not `agent_id="human"`.** Using `human` (Privileged trust) would always have passed the old gate and would not validate that the fix enables the Restricted+Write path. The integration test is a gate-fix regression test; it must exercise the path that was broken. See FR-05.2 steps 2–4.

**C-02 (from SR-03, Medium risk, hard constraint): `force=True` is mandatory on every `context_cycle_review` call in both integration tests.** It is not a recommendation or a default. Using `force=False` (the default) causes the handler to return a cached result from a prior run, which may not contain the stale-edge finding seeded in the test. Omitting it produces a vacuously passing test.

**C-03: Observation seeding must use the same `cycle_id` as the entry setup.** `_seed_observation_sql(db_path, [cycle_id])` must receive the identical `cycle_id` string bound at the top of the test function. A mismatched cycle ID causes `context_cycle_review` to take the "empty feature cycle" early-exit path, returning no detection report.

**C-04: Minimum 20 observation rows per cycle.** `_seed_observation_sql` default of `num_records=20` is sufficient. The observation data is needed for `context_cycle_review` to produce a detection report rather than the empty-cycle acknowledgment path. Do not call with `num_records=0`.

**C-05: Unique cycle IDs per test function.** Both test functions must generate their own cycle ID using the pattern `f"vnc016-{uuid.uuid4().hex[:8]}"` (positive path) and `f"vnc016neg-{uuid.uuid4().hex[:8]}"` (negative path). The shared live-server fixture is process-scoped; reusing a cycle ID across tests causes cross-test interference.

**C-06: The `context_correct` call for deprecating entry A does not need `feature_cycle`.** The SQL query (`query_stale_prerequisite_edges_for_cycle`) joins `feature_entries` on the source entry's ID (A), not the successor entry's ID. Only entry A needs to be in `feature_entries`, and that is satisfied by step 4 of the positive-path test.

**C-07: No new test files.** Tests must be added to the existing `test_tools.py` in the vnc-015 section. No `test_detection.py` or any other new file.

**C-08: No Rust test infrastructure changes.** The Rust unit test (FR-02) uses only the existing store-layer test helpers in `read.rs`. No new fixtures, feature flags, or test-support crates are introduced.

**C-09: `uds_client.py` is not modified.** See FR-03.5.

**C-10: The `feature_entries.feature_id` column name is stable.** The SQL fix in FR-01 relies on this column name being `feature_id`. If a future schema migration renames it, AC-04 would re-introduce the bug. No migration is planned; this assumption is recorded for implementer awareness.

**C-11: `write_capable` has no default value.** It is not `#[serde(default)]` and `UsageContext` does not derive `Default`. Every call site must set it explicitly. This is intentional: the field represents a capability decision that must be affirmative, not a fallback.

**C-12: The gate fix in `record_hook_injection` is required even though the integration test does not exercise the hook injection path.** The two gate blocks (`record_mcp_usage` and `record_hook_injection`) are identical in structure and both contain the broken trust-level check. Fixing only `record_mcp_usage` would leave a latent bug. Both must be fixed together.

**C-13: `write_capable: true` is set unconditionally in the `context_store` handler, not conditionally on `feature_cycle` being present.** The `UsageContext` construction at tools.rs ~826 only runs when `feature_cycle` is `Some` (the `if let Some(fc) = usage_feature_cycle` guard). Within that branch, `write_capable: true` is always correct because `require_cap(Write)` already passed. No additional conditional is needed.

---

## Dependencies

| Dependency | Role | Notes |
|------------|------|-------|
| `crates/unimatrix-store/src/read.rs` | SQL fix target (FR-01) | `query_stale_prerequisite_edges_for_cycle` at line 1607 |
| `crates/unimatrix-store/src/analytics.rs` | Confirms `feature_entries.feature_id` column name | Line 687 |
| `crates/unimatrix-store/src/write_ext.rs` | `record_feature_entries` write path | Line 266 |
| `crates/unimatrix-observe/src/detection/scope.rs` | `DependencyOnDeprecatedRule` — confirmed `rule_name = "dependency_on_deprecated"` | Line 286 |
| `crates/unimatrix-server/src/mcp/tools.rs` | `StoreParams.feature_cycle: Option<String>` (no `#[serde(default)]`); `context_store` handler with `require_cap(Write)` at line 653; `UsageContext` construction at line ~826; `context_cycle_review` handler at line 2165 | Gate fix target; confirms `write_capable: true` placement |
| `crates/unimatrix-server/src/services/usage.rs` | Gate fix target: `UsageContext` struct (lines 50–76); `record_mcp_usage` gate (lines 207–218); `record_hook_injection` gate (lines 272–283) | Both gate blocks must be replaced with `ctx.write_capable` check |
| `product/test/infra-001/harness/client.py` | Harness client to extend (FR-03) | `context_store` at line 383 |
| `product/test/infra-001/suites/test_tools.py` | Integration test target (FR-05, FR-06) | vnc-015 section starts at line 3048; `_seed_observation_sql` at line 967 |
| `product/test/infra-001/suites/test_lifecycle.py` | Reference: `test_stale_dependency_appears_in_context_status` (line 2772) — structural template | Read-only reference |
| `uuid` (Python stdlib-adjacent) | `uuid.uuid4().hex[:8]` for unique cycle ID and agent ID suffixes | Already used in existing tests |

---

## NOT in Scope

- Changes to `DependencyOnDeprecatedRule` logic or its existing unit tests in `scope.rs` — those are complete and passing.
- Changes to `default_rules()` or `detect_hotspots()` in `unimatrix-observe`.
- Testing any detection rule other than `dependency_on_deprecated` end-to-end.
- Testing `context_edge` redirect or remove modes.
- Changes to the `context_cycle_review` cache path (force=true is used to bypass it, not to modify it).
- Performance or load testing.
- Any new MCP tool, parameter, or schema change beyond the `feature_cycle` harness client keyword argument.
- Extending `uds_client.py`.
- Back-filling `feature_entries` rows for entries created before this feature.
- Testing other rules (scope creep from extending observation-seeding).
- Removing the `trust_level` field from `UsageContext` — it is retained for other uses; the scope is limited to the feature-recording gate only.
- Changing trust-level gating anywhere other than the `feature_recording` block in `record_mcp_usage` and `record_hook_injection`.
- Escalating the trust level of the test agent (it must remain Restricted to validate the fix).

---

## Open Questions

None. All open questions from SCOPE.md are resolved:

- OQ-01 (RESOLVED YES): Rust unit test for `query_stale_prerequisite_edges_for_cycle` is required (AC-09, FR-02).
- OQ-02 (RESOLVED NO): `uds_client.py` is not extended (FR-03.5, C-09).
- OQ-03 (RESOLVED NO): `context_correct` call does not need `feature_cycle`; the SQL query only checks the source entry A (C-06).

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned lessons on capability enforcement (id 4444, vnc-015 AC-15 failure from missing agent_id), capability gate changes (id 4411, vnc-014 quarantine cap change), and ADR-007 enforcement-point architecture (id 83). These confirm the pattern: gate fixes require both the Rust change and an integration test that exercises the fixed path with the correct agent class. No new generalizable patterns produced beyond what the retro can promote.
