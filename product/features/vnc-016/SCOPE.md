# vnc-016: End-to-End Integration Test — DependencyOnDeprecated Detection Rule

## Problem Statement

AC-12 of vnc-015 was marked PARTIAL. Unit tests in `unimatrix-observe` verify that
`DependencyOnDeprecatedRule` fires correctly when injected with stale edge pairs, but
no test verifies the full wiring from the MCP layer through to the detection result:

1. `context_cycle_review` handler calls `query_stale_prerequisite_edges_for_cycle`
2. The result is passed as `stale_edge_pairs` into `default_rules()`
3. `DependencyOnDeprecatedRule::new()` receives that data
4. `detect_hotspots()` returns a finding with `rule_name = "dependency_on_deprecated"`
5. That finding appears in the `context_cycle_review` MCP response

A wiring defect in any of those steps produces a silent false-negative: no finding,
no error, no log at INFO level — only a `tracing::warn!` that the integration harness
does not observe. Code reading reveals such a defect is present (see Background Research).

## Goals

1. Add a single end-to-end integration test in the infra-001 Python suite that seeds
   the necessary state, calls `context_cycle_review`, and asserts the
   `dependency_on_deprecated` finding is present in the JSON response.
2. Surface and fix the confirmed wiring defect in
   `query_stale_prerequisite_edges_for_cycle` that would cause the test to fail
   without a production fix.
3. Fix the production bug in `usage.rs` where the `feature_recording` eligibility gate
   checks trust level (`System | Privileged | Internal`) instead of Write capability.
   Any agent with Write capability should be able to tag entries to a feature cycle.
   Currently, a Restricted-trust agent explicitly granted Write capability silently loses
   all `feature_entries` rows — the entry is stored, the cycle tag is dropped. This gate
   appears in both `record_access` and `record_hook_injection`.
4. Extend the harness client (`harness/client.py`) to expose the `feature_cycle`
   parameter on `context_store`, which is required to tag entries to a cycle in
   `feature_entries`.
5. Leave all existing tests passing (non-regression).

## Non-Goals

- Changes to `DependencyOnDeprecatedRule` logic or unit tests — those are complete.
- Changes to `default_rules()` or `detect_hotspots()` — no modifications needed.
- Testing any other detection rule end-to-end — scope is limited to
  `dependency_on_deprecated`.
- Testing the `context_edge` redirect or remove modes — `add` mode suffices.
- Changing the `context_cycle_review` cache path (force=true is used to bypass
  memoization and ensure a fresh detection run).
- Performance or load testing.
- Any new MCP tool, parameter, or schema change beyond the `feature_cycle` harness
  client addition.

## Background Research

### Confirmed Wiring Defect

`crates/unimatrix-store/src/read.rs:1618` — `query_stale_prerequisite_edges_for_cycle`
uses `fe.feature_cycle` in the WHERE clause:

```sql
JOIN feature_entries fe ON fe.entry_id = ge.source_id
WHERE ge.relation_type = 'Prerequisite'
  AND e.status = 1
  AND fe.feature_cycle = ?1   -- BUG: column is named feature_id, not feature_cycle
```

The `feature_entries` table schema (`db.rs:616-621`) defines `feature_id TEXT NOT NULL`
as the feature cycle column. Every write path confirms this:
- `write_ext.rs:274`: `INSERT OR IGNORE INTO feature_entries (feature_id, entry_id, phase)`
- `analytics.rs:687`: same column name
- `write_ext.rs:743,778,816`: test queries use `WHERE feature_id = ...`

SQLite will throw `no such column: fe.feature_cycle` at runtime. The handler catches this
via `unwrap_or_else` (tools.rs:2169-2177) and silently returns `vec![]` with only a
`tracing::warn!`. The rule never fires. This is precisely the defect AC-12 (PARTIAL)
warned about.

**Fix required**: Change `fe.feature_cycle` to `fe.feature_id` in `read.rs:1618`.

### Harness Client Gap

`harness/client.py` `context_store()` (line 383-414) does not expose the `feature_cycle`
parameter. The MCP tool's `StoreParams` struct (tools.rs:143) accepts it. Without this
parameter, `feature_entries` cannot be populated for the source entry (A) via the MCP
call path, forcing direct SQL seeding. Adding `feature_cycle` to the harness client is
the cleanest approach and also fills a general gap useful beyond this feature.

### Query Semantics — What Needs to be in feature_entries

The SQL query joins `feature_entries` on `fe.entry_id = ge.source_id`, scoped to
`fe.feature_id = <cycle>`. This means the DEPRECATED SOURCE ENTRY (A) must appear in
`feature_entries` under the queried cycle. In the issue's prescribed scenario, it is
entry A (the source of the Prerequisite edge) that must be tagged to the cycle, not
entry B (the target). Passing `feature_cycle=<cycle>` to the `context_store` call that
creates A satisfies this requirement.

### Production Bug: feature_recording Trust Gate (usage.rs)

`crates/unimatrix-server/src/services/usage.rs` contains an identical broken gate in
two places (`record_access:208-218` and `record_hook_injection:273-283`):

```rust
let trust = ctx.trust_level.unwrap_or(TrustLevel::Restricted);
if matches!(trust, TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal) {
    Some((feature_str, entry_ids.to_vec()))   // feature_entries written
} else {
    None                                       // silently dropped
}
```

`TrustLevel::Restricted` is the default for auto-enrolled agents and can be explicitly
assigned to agents that are then granted `Capability::Write` by an Admin. Any such agent
calling `context_store` with a `feature_cycle` parameter will have the cycle tag silently
dropped — `feature_entries` is never written, the entry exists but is not attributed to
the cycle, and no error is produced.

`UsageContext` does not carry a capabilities field — it only has `trust_level`. The fix
is to add `write_capable: bool` to `UsageContext` and replace the trust-level match with
`ctx.write_capable`. At the `context_store` handler call site (tools.rs:826), `write_capable`
is set `true` — `require_cap(Capability::Write)` at line 653 already verified it. The
same pattern applies to `record_hook_injection` which shares the identical gate.

**Note**: The integration test must enroll an agent with `trust_level="restricted"` and
`capabilities=["write"]` and use that agent for `context_store` calls. Using
`agent_id="human"` (Privileged) would test a path that always worked and mask whether the
fix actually enables the Restricted+Write path. The Restricted+Write path is the realistic
production scenario for orchestrator agents.

### Existing Test Patterns (infra-001)

- `test_stale_dependency_appears_in_context_status` (test_lifecycle.py:2772) — near-
  identical setup: store A + B, add Prerequisite edge, deprecate A via `context_correct`,
  call `context_status`. Can be used as a structural template.
- `_seed_observation_sql` (test_tools.py:967) — direct SQL seeding for sessions +
  observations. `context_cycle_review` requires at least one observation to produce a
  report (otherwise it returns an "empty feature cycle" error path).
- `_seed_cycle_events_sql` (test_tools.py:1536) — optional SQL seeding for CYCLE_EVENTS.
  Not required for the stale-edge detection path (the rule fires regardless of
  CYCLE_EVENTS rows), but may be added for realism.
- `_query_graph_edges` (test_tools.py:3055) — direct SQLite helper to verify edge writes.
  Not required for assertion but useful for diagnostic context on failure.
- `extract_entry_id`, `assert_tool_success`, `get_result_text` — standard assertion
  helpers available across the suite.

### Caching Consideration

`context_cycle_review` memoizes results (`get_cycle_review` / `store_cycle_review`).
The `force=True` parameter bypasses the cache and forces a fresh detection run. Tests
must pass `force=True` to guarantee the detection pipeline runs, not a stale cached
result.

### Observation Data Requirement

`context_cycle_review` has three lookup paths (col-024). If no observations exist for
the feature cycle, it returns a short acknowledgment, not a detection report. The test
must seed at least one observation session via `_seed_observation_sql` with the same
cycle ID.

### vnc-015 Testing Artifacts

`product/features/vnc-015/testing/` exists. vnc-015 tests live in `test_tools.py`
(section starting at line 3048) and `test_lifecycle.py`. No separate `test_detection.py`
file exists.

## Proposed Approach

**One test function in `test_tools.py`** (extending the vnc-015 section), following the
established pattern for feature-specific integration tests.

**Setup sequence** (maps directly to the issue's prescribed steps):
1. Store entry A with `feature_cycle=<cycle-id>` (requires harness client extension).
2. Store entry B (no special parameters needed).
3. Add Prerequisite edge A→B via `server.context_edge("add", id_a, "Prerequisite", id_b)`.
4. Deprecate A via `server.context_correct(id_a, ...)` (makes source deprecated, edge stale).
5. Seed observation data for the cycle via `_seed_observation_sql(db_path, [cycle_id])` so
   `context_cycle_review` can produce a report.
6. Call `server.context_cycle_review(cycle_id, agent_id="human", format="json", force=True, timeout=30.0)`.
7. Parse JSON response; assert `hotspots` contains an entry with
   `rule_name == "dependency_on_deprecated"`.

**Production fix 1**: Change `fe.feature_cycle` to `fe.feature_id` in `read.rs:1618`.

**Production fix 2**: Add `write_capable: bool` to `UsageContext` in `usage.rs`. Replace
the trust-level gate in both `record_access` and `record_hook_injection` with
`if ctx.write_capable`. Set `write_capable: true` in the `context_store` handler's
`UsageContext` construction (tools.rs:826), where `require_cap(Write)` has already run.

**Harness extension**: Add `feature_cycle: str | None = None` to `context_store()` in
`harness/client.py` (and propagate to `args` dict when non-None).

## Acceptance Criteria

- AC-01: A new integration test `test_dependency_on_deprecated_e2e` exists in
  `product/test/infra-001/suites/test_tools.py` and passes against the production server.
- AC-02: The test follows the prescribed 7-step scenario from GH#601 exactly:
  store A (with feature_cycle), store B, add Prerequisite edge A→B, deprecate A,
  seed observations, call `context_cycle_review(force=True)`, assert finding.
- AC-03: The assertion checks for `rule_name == "dependency_on_deprecated"` in the
  `hotspots` array of the JSON response.
- AC-04: `query_stale_prerequisite_edges_for_cycle` in `read.rs` is fixed: `fe.feature_cycle`
  replaced with `fe.feature_id`.
- AC-05: `harness/client.py` `context_store()` accepts and forwards an optional
  `feature_cycle` keyword argument to the MCP call.
- AC-06: All existing integration tests continue to pass (no regressions).
- AC-07: The test uses `force=True` on `context_cycle_review` to bypass memoization.
- AC-08: A negative-path companion test `test_dependency_on_deprecated_no_finding_without_stale_edge`
  verifies that `context_cycle_review` does NOT emit `dependency_on_deprecated` when
  no Prerequisite edge to a deprecated entry exists for the cycle. This prevents false
  positives from masking a broken always-fires implementation.
- AC-09: A Rust unit test in `unimatrix-store` tests `query_stale_prerequisite_edges_for_cycle`
  directly: seeds `feature_entries` with a known `feature_id`, writes a Prerequisite
  graph edge with a deprecated source entry, calls the function, and asserts the returned
  `Vec<(u64, u64)>` contains the expected pair. This is the test that would have caught
  the `fe.feature_cycle` column-name bug at the store layer.
- AC-10: `UsageContext` gains a `write_capable: bool` field. The trust-level gate in
  `usage.rs:record_access` and `usage.rs:record_hook_injection` is replaced with
  `if ctx.write_capable`. All existing callers default to `write_capable: false` unless
  they explicitly set it.
- AC-11: The `context_store` handler's `UsageContext` construction (tools.rs) sets
  `write_capable: true` — `require_cap(Capability::Write)` at the top of the handler
  has already confirmed this.
- AC-12: The integration test enrolls a Restricted-trust agent with Write capability and
  uses that agent (not `agent_id="human"`) for the `context_store` call that tags entry A
  to the feature cycle. This validates the fixed code path that real orchestrator agents
  will use.
- AC-13: A regression test confirms that a Restricted-trust agent WITHOUT Write capability
  cannot reach `context_store` at all (it is rejected by `require_cap`), so the
  `write_capable: false` default is never exercised on that path.

## Constraints

- The test must run against the live MCP server (infra-001 harness), not a mocked store.
  The defect lives in the SQL layer; a mock would not surface it.
- `context_cycle_review` requires observation data to return a detection report.
  The test must seed at least 20 observation rows via `_seed_observation_sql`.
- The harness client (`client.py`) change must remain backward-compatible: `feature_cycle`
  must be optional with default `None`.
- No new test files — extend the existing `test_tools.py` file in the vnc-015 section.
- No Rust test infrastructure changes beyond the `read.rs` SQL fix and any unit test that
  directly tests `query_stale_prerequisite_edges_for_cycle`.
- The cycle ID used in the test must be unique (use `uuid.uuid4().hex[:8]` suffix pattern
  established by existing tests) to avoid cross-test interference in the shared server fixture.
- SQLite `feature_entries` is populated by `record_feature_entries` which is called via
  the analytics write path during `context_store`. The `feature_cycle` parameter must be
  passed at store time; it cannot be back-filled by `context_cycle`.

## Open Questions

- OQ-01: **RESOLVED — YES.** Add a Rust unit test for `query_stale_prerequisite_edges_for_cycle`
  at the store layer (`unimatrix-store`). The test seeds `feature_entries` with a known
  `feature_id`, adds a stale Prerequisite edge (source deprecated, `status = 1`), calls
  `query_stale_prerequisite_edges_for_cycle`, and asserts the returned pairs. This is the
  exact test that would have caught the `fe.feature_cycle` column-name bug — the integration
  test is slower and harder to debug; when the rule silently returns nothing, a store-layer
  unit test isolates the cause without tracing through the full handler.
- OQ-02: **RESOLVED — NO.** Extend only `client.py`, not `uds_client.py`. Production
  sessions establish the feature cycle context by calling `context_cycle` first, which is
  the correct path the UDS client already supports. The `feature_cycle` parameter on
  `context_store` is a lower-level escape hatch; extending `uds_client.py` would encourage
  bypassing the proper session-context path.
- OQ-03: Does the `context_correct` call also need to pass `feature_cycle` to keep the
  deprecated successor in `feature_entries`? The SQL query only checks the source (A),
  so the answer is no — but clarification is useful for implementers who read the query
  semantics in isolation.

## Tracking

https://github.com/dug-21/unimatrix/issues/603
