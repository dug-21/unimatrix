# vnc-016 Implementation Brief: DependencyOnDeprecated End-to-End Integration Test

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-016/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-016/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-016/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-016/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-016/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-016/ALIGNMENT-REPORT.md |

---

## Goal

Close AC-12 (PARTIAL) from vnc-015 by delivering end-to-end integration test coverage for
the `DependencyOnDeprecatedRule` detection path, fixing the confirmed SQLite column-name bug
that caused a silent false-negative in `query_stale_prerequisite_edges_for_cycle`, and fixing
the production `usage.rs` gate that silently dropped `feature_entries` writes for
Restricted-trust agents with explicit Write capability. The feature also adds a Rust unit test
at the store layer as a permanent regression guard and extends the Python harness client to
expose the `feature_cycle` parameter on `context_store`.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| SQL Fix (`read.rs`) | pseudocode/sql-fix.md | test-plan/sql-fix.md |
| Rust Unit Test (`read.rs mod tests`) | pseudocode/rust-unit-test.md | test-plan/rust-unit-test.md |
| Harness Client Extension (`client.py`) | pseudocode/harness-client.md | test-plan/harness-client.md |
| Usage Gate Fix (`usage.rs` + `tools.rs`) | pseudocode/usage-gate-fix.md | test-plan/usage-gate-fix.md |
| Integration Tests (`test_tools.py`) | pseudocode/integration-tests.md | test-plan/integration-tests.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File / Unimatrix Entry |
|----------|-----------|--------|--------------------------|
| Where to place the Rust unit test for `query_stale_prerequisite_edges_for_cycle` | In the existing `mod tests` block at the bottom of `read.rs` (line 1887). Uses `open_test_store` + raw `sqlx::query` against `store.write_pool`, co-located with the function under test. Two tests: positive path (assert pair returned) and negative companion (assert empty without `feature_entries` row). | ADR-001 | `architecture/ADR-001-rust-unit-test-placement.md` |
| How to propagate Write-capability signal into `UsageContext` for the `feature_entries` gate | Add `write_capable: bool` to `UsageContext`. No default, no serde derivation — every construction site must explicitly set it. Set `true` only at the `context_store` handler callsite (after `require_cap(Write)` passes). Replace trust-level match in both `record_mcp_usage` and `record_hook_injection` with `ctx.write_capable`. ADR-007 (Unimatrix #103, crt-001) is superseded by this decision (Unimatrix entry #4451). | ADR-002 | Unimatrix entry #4451 (architect stored decision; no local ADR file) |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/read.rs` | Modify | Line 1618: change `fe.feature_cycle` to `fe.feature_id` in WHERE clause of `query_stale_prerequisite_edges_for_cycle`. Add two `#[tokio::test]` functions in `mod tests` (line 1887+). |
| `crates/unimatrix-server/src/services/usage.rs` | Modify | Add `write_capable: bool` field to `UsageContext` struct. Replace trust-level gate in `record_mcp_usage` (lines 207-218) and `record_hook_injection` (lines 272-283) with `if ctx.write_capable`. Add two unit tests in `mod tests` for gate logic. |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | At `UsageContext` construction site (~line 826, inside `context_store` handler): set `write_capable: true`. At all other `UsageContext` construction sites in this file: set `write_capable: false`. |
| `product/test/infra-001/harness/client.py` | Modify | Add `feature_cycle: str | None = None` keyword parameter to `context_store()` (after `edges`). Add guard: `if feature_cycle is not None: args["feature_cycle"] = feature_cycle`. |
| `product/test/infra-001/suites/test_tools.py` | Modify | Append after vnc-015 section (line 3048+): `test_dependency_on_deprecated_e2e` (positive, 9-step) and `test_dependency_on_deprecated_no_finding_without_stale_edge` (negative). |

No new files are created.

---

## Data Structures

### `UsageContext` (after fix) — `usage.rs`

The `write_capable` field is new. No `Default`, no `#[serde(default)]`. Every construction
site must explicitly set the field — Rust's exhaustive struct construction enforces this.

Fields relevant to this feature:

- `feature_cycle: Option<String>` — existing; the feature cycle tag from the MCP call
- `trust_level: Option<TrustLevel>` — existing; retained but no longer used in the feature_recording gate
- `write_capable: bool` — NEW; `true` only when the caller has been verified by `require_cap(Capability::Write)`

### `feature_entries` table — authoritative schema (`db.rs:616-621`)

| Column | Type | Notes |
|--------|------|-------|
| `feature_id` | `TEXT NOT NULL` | The feature cycle string. Column is named `feature_id`, not `feature_cycle`. |
| `entry_id` | `INTEGER NOT NULL` | Foreign key to `entries.id`. |
| `phase` | `TEXT` | Nullable. |

### `query_stale_prerequisite_edges_for_cycle` return type

`Result<Vec<(u64, u64)>>` — pairs of `(source_entry_id, target_entry_id)` where source is
Deprecated (`status = 1`) and appears in `feature_entries` under the queried cycle.

Tests MUST NOT use `unwrap_or_else(|_| vec![])` — errors must surface as test failures.

### Integration test ID patterns

```python
cycle_id      = f"vnc016-{uuid.uuid4().hex[:8]}"         # positive path
cycle_id      = f"vnc016neg-{uuid.uuid4().hex[:8]}"       # negative path
test_agent_id = f"vnc016-agent-{uuid.uuid4().hex[:8]}"    # Restricted+Write agent
```

---

## Function Signatures

### SQL Fix target — `read.rs:1607`

```rust
pub async fn query_stale_prerequisite_edges_for_cycle(
    &self,
    feature_cycle: &str,
) -> Result<Vec<(u64, u64)>>
```

Signature is unchanged. Fix is internal: `fe.feature_cycle = ?1` becomes `fe.feature_id = ?1`
on line 1618.

### Rust unit tests — `read.rs mod tests`

```rust
#[tokio::test]
async fn test_query_stale_prerequisite_edges_for_cycle_returns_pair()
// Seeds: entry A (status=1 Deprecated), entry B (status=0 Active),
//        feature_entries(feature_id=<cycle>, entry_id=A.id, phase=NULL),
//        graph_edges(source_id=A.id, target_id=B.id, relation_type='Prerequisite').
// Asserts: result.is_ok() AND result.unwrap().len() == 1 AND result.unwrap()[0] == (A.id, B.id)

#[tokio::test]
async fn test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry()
// Seeds: same entries and edge, NO feature_entries row for any cycle.
// Asserts: result.is_ok() AND result.unwrap().is_empty()
```

Both tests use `open_test_store(&dir)` and raw `sqlx::query` against `store.write_pool`,
matching the pattern of `test_query_graph_edges_returns_rows` (line 2056).

### Usage gate replacement — `usage.rs`

```rust
// In both record_mcp_usage (lines 207-218) and record_hook_injection (lines 272-283):
// REMOVE the trust-level match entirely; REPLACE with:
let feature_recording = ctx.feature_cycle.and_then(|feature_str| {
    if ctx.write_capable {
        Some((feature_str, entry_ids.to_vec()))
    } else {
        None
    }
});
```

### Usage gate unit tests — `usage.rs mod tests`

```rust
#[test]
fn test_write_capable_false_yields_no_feature_recording()
// UsageContext { write_capable: false, feature_cycle: Some("test-cycle".into()), ... }
// Gate result: None

#[test]
fn test_write_capable_true_yields_feature_recording()
// UsageContext { write_capable: true, feature_cycle: Some("test-cycle".into()), ... }
// Gate result: Some(...)
```

### Harness client extension — `client.py:context_store`

```python
def context_store(
    self,
    content: str,
    topic: str,
    category: str,
    # ... existing params ...
    edges: list | None = None,
    feature_cycle: str | None = None,   # NEW — keyword-only, default None
) -> MCPResponse:
    args = { ... }  # existing args construction unchanged
    if feature_cycle is not None:
        args["feature_cycle"] = feature_cycle
    return self.call_tool("context_store", args)
```

### Integration test assertions — `test_tools.py`

```python
# Positive path (must be True after fix; would be False without fix)
data = json.loads(get_result_text(resp))
assert "hotspots" in data
assert any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"])

# Negative path (must always be False; guards against always-fires regression)
assert not any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"])
```

---

## Constraints

All constraints are hard requirements, not recommendations.

**C-01 (Critical — SR-02)**: `feature_cycle` must be passed on the `context_store` call for
entry A. `record_feature_entries` runs at write time only. No back-fill path exists. Omitting
`feature_cycle` at step 4 produces a vacuously passing test with an empty `feature_entries`.

**C-01b (Critical)**: The `context_store` call for entry A in the positive-path test MUST use
the enrolled Restricted+Write agent (`test_agent_id`), NOT `agent_id="human"`. Using "human"
(Privileged) exercises a path that always passed the old gate and provides no regression signal.

**C-02 (Critical — SR-03)**: `force=True` is mandatory on every `context_cycle_review` call
in both integration tests. A cached result bypasses the detection pipeline silently.

**C-03**: `_seed_observation_sql` must receive the identical `cycle_id` bound at the top of
the test function. A mismatched ID triggers the empty-cycle early-exit, not a detection report.

**C-04**: Minimum 20 observation rows per cycle (`num_records=20` default is sufficient).

**C-05**: Both integration test functions generate their own `cycle_id` via `uuid.uuid4().hex[:8]`
suffix. Cycle IDs are never reused across test functions.

**C-06**: The `context_correct` call to deprecate entry A does NOT need `feature_cycle`. The
SQL query joins on source entry membership in `feature_entries`, already satisfied by C-01.

**C-07**: No new test files. Extend `test_tools.py` only, after the vnc-015 section.

**C-08**: No new Rust test infrastructure. Use the existing `open_test_store` + `insert_test_entry`
pattern from `read.rs mod tests`.

**C-09**: `uds_client.py` is NOT modified.

**C-10**: `feature_entries.feature_id` column name is assumed stable. The SQL fix depends on it.

**C-11**: `write_capable` has no `Default` derivation and no `#[serde(default)]`. Every
`UsageContext` construction site must explicitly set the field. Omitting the field is a
compile error.

**C-12**: Both gate blocks — `record_mcp_usage` AND `record_hook_injection` — must be fixed.
Both contain the identical broken trust-level check. Fixing only one leaves a latent bug.

**C-13**: `write_capable: true` is set unconditionally in the `context_store` handler's
`UsageContext` construction block (inside the `if let Some(fc) = usage_feature_cycle` branch).
Within that branch, `require_cap(Write)` has already passed; `true` is always correct.

---

## Dependencies

| Dependency | Role | Location |
|------------|------|----------|
| `crates/unimatrix-store/src/read.rs` | SQL fix target + Rust unit tests | Line 1618 (fix); line 1887+ (tests) |
| `crates/unimatrix-store/src/db.rs` | Authoritative `feature_entries` schema | Lines 616-621 |
| `crates/unimatrix-store/src/write_ext.rs` | `record_feature_entries` write path | Line 274 |
| `crates/unimatrix-store/src/analytics.rs` | Confirms `feature_id` column name | Line 687 |
| `crates/unimatrix-observe/src/detection/scope.rs` | `DependencyOnDeprecatedRule::name()` returns `"dependency_on_deprecated"` | Line 286 |
| `crates/unimatrix-server/src/mcp/tools.rs` | `StoreParams.feature_cycle` (line 143); `require_cap(Write)` (line 653); `UsageContext` construction (~line 826); `context_cycle_review` handler (lines 2165-2177) | Gate fix; `write_capable: true` callsite |
| `crates/unimatrix-server/src/services/usage.rs` | `UsageContext` struct (lines 50-76); `record_mcp_usage` gate (lines 207-218); `record_hook_injection` gate (lines 272-283) | Both gate blocks must be replaced |
| `product/test/infra-001/harness/client.py` | Harness client to extend | `context_store` at line 383 |
| `product/test/infra-001/suites/test_tools.py` | Integration test target; `_seed_observation_sql` at line 967 | vnc-015 section at line 3048+ |
| `product/test/infra-001/suites/test_lifecycle.py` | Reference only — structural template | `test_stale_dependency_appears_in_context_status` at line 2772 |
| `uuid` (Python) | Unique ID suffix generation | Already used in test suite |
| Unimatrix entry #4451 | ADR-002 — `write_capable` gate decision; supersedes ADR-007 / entry #103 | Stored by architect |

---

## NOT in Scope

- Changes to `DependencyOnDeprecatedRule` logic or existing unit tests in `scope.rs`.
- Changes to `default_rules()` or `detect_hotspots()` in `unimatrix-observe`.
- End-to-end testing for any detection rule other than `dependency_on_deprecated`.
- Testing `context_edge` redirect or remove modes.
- Modifying the `context_cycle_review` memoization or cache path.
- Performance or load testing.
- Any new MCP tool, parameter, or schema change beyond the Python harness client keyword argument.
- Extending `uds_client.py`.
- Back-filling `feature_entries` rows for entries created before this feature.
- Removing the `trust_level` field from `UsageContext`.
- Changing trust-level gating anywhere except the `feature_recording` block in `record_mcp_usage` and `record_hook_injection`.
- Escalating the trust level of the integration test agent (must remain Restricted to validate the fix).
- Hardening `unwrap_or_else` WARN to ERROR in `tools.rs:2169` — deferred; a GitHub issue must be created at PR time and referenced in the PR description.

---

## Alignment Status

ALIGNMENT-REPORT.md: all checks PASS except one WARN.

**WARN (R-05)**: `tools.rs:2169-2177` uses `unwrap_or_else` logging at `tracing::warn!`
when `query_stale_prerequisite_edges_for_cycle` fails. This is not changed by vnc-016 and
remains a latent re-concealment risk for future SQL regressions. The Rust unit test (AC-09)
is the primary regression guard. The delivery agent must create a GitHub issue at PR time for
the WARN->ERROR hardening and reference it in the PR description.

**Unimatrix entry #4451 verification**: Before closing, run `context_get(id=4451)` to confirm
ADR-002 is stored and the supersession chain from entry #103 (ADR-007) is linked correctly.
If the chain is not linked, run `context_correct` to link it.
