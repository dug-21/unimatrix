# vnc-016 Architecture: DependencyOnDeprecated End-to-End Integration Test

## System Overview

vnc-016 closes AC-12 from vnc-015 (PARTIAL). The `DependencyOnDeprecatedRule` has
complete unit coverage in `unimatrix-observe`, but no test exercises the full wiring
from the MCP layer through to the JSON response. A confirmed column-name bug in
`read.rs` (`fe.feature_cycle` vs `fe.feature_id`) causes a silent false-negative that
no existing test would catch.

This feature delivers three coordinated changes:
1. A one-line SQL fix in `crates/unimatrix-store/src/read.rs`.
2. A Rust unit test in `read.rs` that would have caught the bug at the store layer.
3. A Python integration test in `product/test/infra-001/suites/test_tools.py` (with a
   harness client extension) that verifies the full MCP-to-detection wiring.

## Component Breakdown

### Component 1: SQL Fix — `read.rs`

**File**: `crates/unimatrix-store/src/read.rs`, line 1618

**Responsibility**: `query_stale_prerequisite_edges_for_cycle` queries the three-way
join `graph_edges × entries × feature_entries` to find Prerequisite edge pairs whose
source is Deprecated and belongs to a given feature cycle.

**The defect**: The WHERE clause uses `fe.feature_cycle` but `feature_entries` defines
the column as `feature_id` (DDL at `db.rs:616-621`, confirmed in every INSERT path).
SQLite throws a runtime error; the handler swallows it via `unwrap_or_else` and returns
`vec![]` with only `tracing::warn!`.

**The fix**: Change `fe.feature_cycle = ?1` to `fe.feature_id = ?1` on line 1618.
This is a single-token change with no API surface modification.

**Safety**: `query_stale_prerequisite_edges_for_cycle` has exactly two callers:
- `crates/unimatrix-store/src/read.rs` — the definition.
- `crates/unimatrix-server/src/mcp/tools.rs` — the single call site (the
  `context_cycle_review` handler, lines 2165-2177).

No other file references this function (confirmed via grep). The function signature,
return type `Result<Vec<(u64, u64)>>`, and all downstream code remain unchanged.

### Component 2: Rust Unit Test — `read.rs` test module

**File**: `crates/unimatrix-store/src/read.rs`, appended to the existing `mod tests`
block starting at line 1887.

**Responsibility**: Exercise `query_stale_prerequisite_edges_for_cycle` directly
against an in-process SQLite store, asserting the returned `Vec<(u64, u64)>` is
non-empty. This is the test that would have caught the `fe.feature_cycle` column bug at
compile+run time without requiring a full MCP server.

**Test placement rationale**: The function lives in `read.rs`, its test module already
exists (line 1887), and peer tests in that module use the same `open_test_store` +
`write_pool` pattern for seeding. No new test file or separate integration test crate is
needed. Keeping the test co-located with the function it guards follows the established
pattern for all other query functions in `read.rs`.

See ADR-001 for the full placement decision.

**Seeding sequence** (all via raw sqlx against `store.write_pool`):
1. Insert entry A into `entries` with `status = 1` (Deprecated).
2. Insert entry B into `entries` with `status = 0` (Active) — the target.
3. Insert a row into `feature_entries` with `(feature_id, entry_id)` = `(<cycle>, A.id)`.
4. Insert a row into `graph_edges` with `(source_id, target_id, relation_type)` =
   `(A.id, B.id, 'Prerequisite')`.
5. Call `store.query_stale_prerequisite_edges_for_cycle(<cycle>)`.
6. Assert the returned vec contains exactly `(A.id, B.id)`.

**Negative companion** (same module): seed the same entries and edge, but do NOT
insert into `feature_entries` — assert the returned vec is empty. This confirms the
feature cycle scoping works.

### Component 3: Harness Client Extension — `client.py`

**File**: `product/test/infra-001/harness/client.py`, `context_store()` method
(lines 383-414).

**Responsibility**: Forward the `feature_cycle` MCP parameter from the Python test
harness to the `context_store` tool call. This parameter already exists in
`StoreParams` (tools.rs line 143) but was not exposed in the client.

**Change**: Add `feature_cycle: str | None = None` as a keyword-only parameter after
`edges`. Add a guard block:

```python
if feature_cycle is not None:
    args["feature_cycle"] = feature_cycle
```

The `args` dict key is `"feature_cycle"` — matching the `StoreParams` field name
(tools.rs:143). This is the only dict key added.

**Backward compatibility**: `feature_cycle` is optional with `None` default. All
existing `context_store()` call sites are unaffected.

**SR-05 resolution**: `StoreParams.feature_cycle` is `Option<String>` with standard
serde deserialization — a missing JSON key and an explicit `null` both deserialize to
`None`. The `if feature_cycle is not None` guard on the Python side means the key is
absent (not null) when not provided, which is the safe path.

### Component 4: Integration Test — `test_tools.py`

**File**: `product/test/infra-001/suites/test_tools.py`, appended to the vnc-015
section starting at line 3048.

**Responsibility**: Verify the full wiring from MCP tool calls through to the
`dependency_on_deprecated` finding in the JSON response, using the live server.

Two test functions are added (AC-01, AC-08):
- `test_dependency_on_deprecated_e2e` — positive path (finding present).
- `test_dependency_on_deprecated_no_finding_without_stale_edge` — negative path
  (finding absent when no stale edge for the cycle exists).

## Component Interactions

```
test_tools.py
    │
    ├─ client.py:context_store(feature_cycle=<cycle>)
    │       → StoreParams.feature_cycle → UsageService.record_access()
    │               → feature_entries (feature_id = <cycle>, entry_id = A)
    │
    ├─ client.py:context_edge("add", A, "Prerequisite", B)
    │       → graph_edges (source_id=A, target_id=B, relation_type='Prerequisite')
    │
    ├─ client.py:context_correct(A, ...)
    │       → entries.status = 1 (Deprecated) for entry A
    │
    ├─ _seed_observation_sql(db_path, [cycle_id], num_records=20)
    │       → sessions + observations tables
    │
    └─ client.py:context_cycle_review(cycle, force=True, format="json")
            → tools.rs handler
                    → query_stale_prerequisite_edges_for_cycle(cycle)
                            → JOIN graph_edges × entries × feature_entries
                            → WHERE fe.feature_id = cycle   [AFTER FIX]
                            → returns [(A, B)]
                    → default_rules(history, [(A, B)])
                            → DependencyOnDeprecatedRule::new([(A, B)])
                    → detect_hotspots(...)
                    → JSON response: hotspots[0].rule_name = "dependency_on_deprecated"
```

## Analytics Write Path (SR-02 Analysis)

The path from `context_store(feature_cycle=<cycle>)` to `feature_entries` row:

1. `tools.rs` handler (lines 739-744): `usage_feature_cycle` is set from
   `params.feature_cycle` (caller-supplied) or falls back to
   `feature_cycle_from_session`. When `params.feature_cycle = Some(<cycle>)`, it is
   used directly.

2. Handler calls `self.services.usage.record_access(...)` with
   `UsageContext { feature_cycle: Some(<cycle>), trust_level: Some(ctx.trust_level), ... }`
   (lines 822-836).

3. `UsageService::record_access()` (usage.rs lines 207-218): **FIXED by this feature.**
   The old gate checked `trust_level matches System | Privileged | Internal`, silently
   dropping feature_entries for Restricted-trust agents with Write capability. The fix
   replaces this with `if ctx.write_capable`, where `write_capable: bool` is a new field
   on `UsageContext` set `true` at the `context_store` call site (after `require_cap(Write)`
   has already passed). See the "Production Bug Fix" section and ADR-002 for full details.

4. `record_feature_entries(&feature_str, &ids, phase_snapshot)` writes
   `INSERT OR IGNORE INTO feature_entries (feature_id, entry_id, phase)` (write_ext.rs
   line 274). The column name is `feature_id`, not `feature_cycle`.

5. The write is async (tokio::spawn) but completes before `context_cycle_review` is
   called because the test issues sequential MCP calls with no concurrency.

**Conclusion**: The MCP path fully populates `feature_entries` when `feature_cycle` is
passed to `context_store` by an agent with Write capability. No direct SQL seeding is
needed for entry A's `feature_entries` row. The integration test uses a
Restricted+Write enrolled agent (not `agent_id="human"`) — see AC-12 and the
"Integration Test Structure" section below.

## Integration Points

### Existing Interfaces Used

| Interface | Signature / Type | Defined In |
|-----------|-----------------|------------|
| `query_stale_prerequisite_edges_for_cycle` | `async fn(&self, feature_cycle: &str) -> Result<Vec<(u64, u64)>>` | `read.rs:1607` |
| `StoreParams.feature_cycle` | `Option<String>` (serde field) | `tools.rs:143` |
| `_seed_observation_sql` | `(db_path, feature_ids, num_records=20) -> list[(fid, session_id)]` | `test_tools.py:967` |
| `assert_tool_success` | `(MCPResponse) -> None` | `test_tools.py` helpers |
| `extract_entry_id` | `(MCPResponse) -> int` | `test_tools.py` helpers |
| `get_result_text` | `(MCPResponse) -> str` | `test_tools.py` helpers |
| `open_test_store` | `async fn(dir: &TempDir) -> SqlxStore` | `test_helpers.rs:13` |
| `Status::Deprecated` | `= 1i64` (SQLite encoding) | `schema.rs:12` |

### New Interfaces Introduced

| Interface | Signature / Type | Introduced In |
|-----------|-----------------|---------------|
| `client.py:context_store(feature_cycle=)` | `str \| None = None` keyword arg | `client.py` |
| `test_dependency_on_deprecated_e2e` | pytest function `(server)` | `test_tools.py` |
| `test_dependency_on_deprecated_no_finding_without_stale_edge` | pytest function `(server)` | `test_tools.py` |
| `test_query_stale_prerequisite_edges_for_cycle_returns_pair` | `#[tokio::test] async fn` | `read.rs mod tests` |
| `test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry` | `#[tokio::test] async fn` | `read.rs mod tests` |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `feature_entries.feature_id` | `TEXT NOT NULL` | `db.rs:616-621`, `write_ext.rs:274` |
| `feature_entries.entry_id` | `INTEGER NOT NULL` | `db.rs:618` |
| `graph_edges.relation_type` | `TEXT` — must be `'Prerequisite'` | `read.rs:1616` |
| `entries.status` | `INTEGER` — `1 = Deprecated` | `schema.rs:12` |
| `context_cycle_review.force` | `bool` — bypasses memoization | `client.py:650-659` |
| `context_cycle_review` JSON response | `hotspots[].rule_name: str` | `tools.rs` response path |
| `args["feature_cycle"]` | `str` — dict key in Python args | `client.py context_store` |
| `UsageContext.write_capable` | `bool` — `true` only when `require_cap(Write)` has passed at call site | `usage.rs` (ADR-002) |

## Integration Test Structure

### Positive Test: `test_dependency_on_deprecated_e2e`

```
cycle_id = f"vnc016-{uuid.uuid4().hex[:8]}"   # unique per invocation
test_agent = f"vnc016-test-{uuid.uuid4().hex[:6]}"

# Enroll a Restricted+Write agent (Admin op, human has Admin by bootstrap)
server.context_enroll(test_agent, trust_level="restricted",
                      capabilities=["write", "read"], agent_id="human")

Step 1: resp_a = server.context_store(..., feature_cycle=cycle_id, agent_id=test_agent)
        id_a = extract_entry_id(resp_a)
        # write_capable=True (require_cap(Write) passed) → feature_entries written async

Step 2: resp_b = server.context_store(..., agent_id="human")
        id_b = extract_entry_id(resp_b)

Step 3: server.context_edge("add", id_a, "Prerequisite", id_b, agent_id="human")
        # graph_edges: (source_id=id_a, target_id=id_b, relation_type='Prerequisite')

Step 4: server.context_correct(id_a, "<successor content>", agent_id="human")
        # entries.status = 1 for id_a

Step 5: db_path = _resolve_db_path(server.project_dir)
        _seed_observation_sql(db_path, [cycle_id], num_records=20)

Step 6: resp = server.context_cycle_review(cycle_id, agent_id="human",
                   format="json", force=True, timeout=30.0)

Step 7: data = json.loads(get_result_text(resp))
        hotspots = data["hotspots"]
        rule_names = [h["rule_name"] for h in hotspots]
        assert "dependency_on_deprecated" in rule_names
```

**Why `test_agent` and not `agent_id="human"` for Step 1**: `human` has Privileged trust,
which passed the old broken gate. Using it would make the test pass before and after the
fix, proving nothing. `test_agent` has Restricted trust — the path that was silently broken.
The test fails against the unfixed code and passes against the fixed code (AC-12).

### Negative Test: `test_dependency_on_deprecated_no_finding_without_stale_edge`

Same cycle_id pattern. Stores A and B with a Prerequisite edge, but does NOT
deprecate A. Seeds observations for the same cycle_id. Calls
`context_cycle_review(cycle_id, force=True, format="json")`. Asserts
`"dependency_on_deprecated"` is NOT in the `rule_names` list.

This guards against an always-fires implementation that would make the positive test
vacuously pass.

### Memoization Bypass (AC-07, SR-03)

`force=True` is mandatory on every `context_cycle_review` call in both tests. The
parameter is already exposed in `client.py:context_cycle_review` (line 650). Omitting
`force=True` risks hitting a cached result from a prior test run in the same server
process.

## Known Architectural Constraint: Error Swallowing (SR-01)

`tools.rs:2169-2177` uses `unwrap_or_else` to silently swallow any
`query_stale_prerequisite_edges_for_cycle` failure, returning `vec![]` with only a
`tracing::warn!`. This is the pattern that concealed the column-name bug.

This is a known, intentional architectural choice: the handler degrades gracefully
rather than returning an error when detection data is unavailable. The consequence is
that any future regression in the SQL (column rename, schema drift) will again produce
a silent false-negative.

**Mitigation**: The Rust unit test (AC-09) is the primary regression guard. It calls
the function directly against a real SQLite store and asserts a non-empty `Vec`. The
integration test provides end-to-end coverage but is slower and harder to debug. Both
layers are required.

The `unwrap_or_else` pattern itself is not changed by vnc-016 — modifying the error
handling strategy is out of scope and would require its own design review.

## Production Bug Fix: `usage.rs` Trust-Level Gate (AC-10 through AC-13)

### The Defect

`UsageService::record_mcp_usage()` and `UsageService::record_hook_injection()` both
contain an identical eligibility gate for writing `feature_entries` rows:

```rust
let feature_recording = ctx.feature_cycle.and_then(|feature_str| {
    let trust = ctx.trust_level.unwrap_or(TrustLevel::Restricted);
    if matches!(trust, TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal) {
        Some((feature_str, entry_ids.to_vec()))
    } else {
        None   // silently dropped
    }
});
```

`TrustLevel::Restricted` is excluded. An agent enrolled with `trust_level=Restricted`
and explicitly granted `Capability::Write` by an Admin will have all `feature_entries`
writes silently dropped when it calls `context_store` with a `feature_cycle` parameter.
The entry is stored; the cycle tag is lost. No error is produced.

This is a behavioral contract violation: `Capability::Write` is the gate for
`context_store` — once an agent passes that capability check, all effects of a store
call (including cycle attribution) must execute regardless of trust level.

### Root Cause: Missing Capability Signal in `UsageContext`

`UsageContext` carries `trust_level: Option<TrustLevel>` but no capability field.
`ToolContext` (the per-request context available to handlers) also does not carry
capabilities — it records `trust_level` only (context.rs lines 16-36). The handlers
therefore had no way to propagate capability information into `UsageContext`, so the
original implementation fell back to trust level as a proxy — which is incorrect.

### The Fix: `write_capable: bool` in `UsageContext`

Add one field to `UsageContext` in `usage.rs`:

```rust
/// Whether the caller passed Capability::Write for this call.
/// Set true only at the context_store call site (after require_cap(Write) passes).
/// All other UsageContext construction sites default to false.
pub write_capable: bool,
```

Replace both trust-level gates with:

```rust
let feature_recording = ctx.feature_cycle.and_then(|feature_str| {
    if ctx.write_capable {
        Some((feature_str, entry_ids.to_vec()))
    } else {
        None
    }
});
```

At the `context_store` handler's `UsageContext` construction site (tools.rs ~line 826),
set `write_capable: true`. This is safe because `require_cap(Capability::Write)` at
line 653 of the same handler has already returned `Ok(())` before reaching the
`UsageContext` construction. The capability check is the authority; `write_capable`
is simply a signal that the check passed.

All other `UsageContext` construction sites throughout the codebase set
`write_capable: false` (or omit it — which is a compile error for a non-`Default` struct,
enforcing deliberateness at each call site).

See ADR-002 for the full decision rationale and alternatives considered.

### Why `agent_id="human"` Was Wrong as a Test Vehicle

The original architecture document (Analytics Write Path section) noted:

> The test uses `agent_id="human"`, which resolves to `TrustLevel::Privileged`
> (registry bootstrap default, confirmed in `infra/registry.rs`). The guard passes.

This is precisely the problem: `human` is `TrustLevel::Privileged`, which was always
in the passing set `(System | Privileged | Internal)`. A test using `agent_id="human"`
exercises a path that worked even before the fix. It provides no signal that the
Restricted+Write path is working.

The production failure mode is a Restricted-trust agent (the realistic case for
orchestrator agents auto-enrolled during delivery) with Write capability having its
cycle attribution silently dropped. That path is invisible when the test uses a
Privileged agent.

### Corrected Integration Test Setup (AC-12)

The integration test must use an agent that exercises the fixed code path:

```
Step 0 (new): server.context_enroll(
    agent_id="vnc016-restricted-writer",
    trust_level="restricted",
    capabilities=["write"],
    agent_id_caller="human"   # Admin agent performing the enrollment
)

Step 1: resp_a = server.context_store(
    ...,
    feature_cycle=cycle_id,
    agent_id="vnc016-restricted-writer"   # Restricted+Write agent
)
id_a = extract_entry_id(resp_a)
# feature_entries: (feature_id=cycle_id, entry_id=id_a) written — only if fix is correct

Steps 2-7: unchanged from original design.
```

The enrollment step must precede every use of the test agent. The `agent_id` used in
Step 1 must be the newly enrolled Restricted+Write agent, not `"human"`. If the fix is
absent, `feature_entries` is not written for id_a, `context_cycle_review` finds no stale
edge, and the test fails — which is the correct regression signal.

### AC-13: Confirming the `write_capable: false` Default Is Never Reached Incorrectly

A Restricted-trust agent without Write capability cannot reach `context_store` at all —
`require_cap(Capability::Write)` at line 653 returns an error and the handler exits
before `UsageContext` is constructed. The `write_capable: false` default therefore only
applies at other `UsageContext` construction sites (search, lookup, get, briefing, etc.)
where `feature_entries` writes are never expected. The regression test asserts that a
Restricted agent lacking Write capability receives a capability error response from
`context_store`.

## Integration Surface (Updated)

The following entries extend the Integration Surface table from the original design:

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `UsageContext.write_capable` | `bool` — `true` only at `context_store` call site | `usage.rs` (new field) |
| `require_cap(Capability::Write)` | Called at `tools.rs:653` before `UsageContext` construction | `tools.rs` |
| `context_enroll` | MCP tool for agent enrollment; Admin capability required | `tools.rs` (enroll handler) |
| `TrustLevel::Restricted` | Default for auto-enrolled agents; previously excluded from gate | `infra/registry.rs` |

## Decisions

| ADR | Title | Unimatrix ID |
|-----|-------|--------------|
| ADR-001 | Rust unit test for `query_stale_prerequisite_edges_for_cycle` lives in `read.rs mod tests` | #4449 |
| ADR-002 | `write_capable: bool` field in `UsageContext` replaces trust-level gate for `feature_entries` writes | #4451 |

## Open Questions

None. All OQs from SCOPE.md are resolved:
- OQ-01: Rust unit test added to `read.rs mod tests` (ADR-001 decision).
- OQ-02: Only `client.py` extended, not `uds_client.py`.
- OQ-03: `context_correct` call for entry A does not need `feature_cycle`; the SQL
  query checks source membership in `feature_entries`, which is set at store time for A.
  The successor entry (created by `context_correct`) does not need to be in
  `feature_entries` for the detection query to return A's edge.
