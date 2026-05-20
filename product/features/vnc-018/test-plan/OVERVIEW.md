# vnc-018 Test Plan: OVERVIEW

## Test Strategy

vnc-018 adds `context_graph` as the 14th MCP tool with three modes (chain, current,
neighbors), four schema indexes (v26→v27 migration), Advances/Motivates PPR/BFS
additions, and two new SQL query functions. Testing spans three layers:

- **Unit tests** (Rust) — in-module `#[cfg(test)]` blocks covering pure functions,
  SQL query functions, validation logic, and schema migration assertions
- **Integration tests** (Rust) — cross-crate functions exercised against a live
  SQLite DB (migration test file, store-layer query tests)
- **Integration harness** (Python/infra-001) — end-to-end MCP protocol tests against
  the compiled binary covering all three modes, protocol tool count, and critical risk scenarios

### Testing Priorities by Layer

| Layer | Primary Targets |
|-------|----------------|
| Unit | validate_no_unsupported_params, handle_chain/current/neighbors, Truncated wire shape, BFS visited-set keying, PPR/BFS additions, node_index_for accessor |
| Integration (Rust) | query_supersession_chain, query_direct_neighbors, migration_v26_to_v27 index assertions, schema cascade |
| Integration (Python) | AC-20 all three modes, R-03 staleness, AC-05a/R-21 asymmetry pair, R-20 orphaned-deprecated, AC-16 tool count |

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test Type | Component | Key Scenarios |
|---------|----------|-----------|-----------|---------------|
| R-01 | Critical | Unit + Rust integration | graph_read, store_queries | Cold-start CTE correctness; `find_terminal_active` not called |
| R-02 | Critical | Unit + Python integration | graph_read | AC-03b raw JSON wire shape of `truncated` struct |
| R-03 | Critical | Python integration | graph_read (neighbors) | depth=1 sees edge immediately; depth=2 does NOT (expected staleness) |
| R-04 | Critical | Unit | graph_read (validate) | Unrecognized mode with forward-compat field: "unrecognized mode" fires first |
| R-05 | Critical | Rust integration | migration | All 7 cascade touch points; AC-19 four index names in sqlite_master |
| R-06 | Critical | Unit + Python | graph_read (neighbors) | AC-15a exact error string; AC-10a raw JSON has no excluded_types field |
| R-07 | Low (resolved ADR-008) | Unit | graph_read (BFS) | node_index_for returns correct NodeIndex; returns None for unknown |
| R-08 | High | Unit | graph_read (validate) | AC-15c exact error for resolve_supersessions on chain mode |
| R-09 | High | Unit | ppr_bfs | AC-17 Advances/Motivates in PPR; AC-18 in graph_expand BFS |
| R-10 | High | Unit + Python | graph_read (neighbors) | follow_to_current None path: graceful fallback, no panic |
| R-11 | High | Unit | graph_read (neighbors) | depth=0 error; depth=11 error; depth=1 and depth=10 valid |
| R-12 | High | Python | graph_read (neighbors) | neighbors non-existent ID returns empty (SCOPE.md OQ-01 resolved) |
| R-13 | High | Python + code review | tools_dispatch | AC-20 runtime dispatch proof; confirm fully-qualified path |
| R-14 | High | Python | tools_dispatch | AC-16 P-03 asserts 14 tools |
| R-15 | Medium | Python | graph_read (neighbors) | EdgeRecord.metadata in raw JSON is null, not absent |
| R-16 | Medium | Unit | graph_read (validate) | AC-15b four forward-compat fields each trigger validation error |
| R-17 | Medium | Unit | graph_read (neighbors) | direction="forward" on neighbors mode → error; incoming/outgoing/both → valid |
| R-18 | Medium | Unit | graph_read (BFS) | AC-11a: node reachable at depth=1 and depth=2 appears once at depth=1 |
| R-19 | Pre-delivery gate | Branch check | — | 16 RelationType variants present; smoke test Advances edge no error |
| R-20 | Critical | Python integration | graph_read (current) | AC-06b: orphaned deprecated entry returns "no active terminal found" |
| R-21 | High | Python integration | graph_read (current+chain) | AC-05a/AC-04 matched pair: current→error, chain→empty for same ID |

---

## Cross-Component Test Dependencies

```
tools_dispatch tests depend on → graph_read (validates dispatch wiring)
graph_read.handle_chain/current depend on → store_queries.query_supersession_chain
graph_read.handle_neighbors(depth=1) depends on → store_queries.query_direct_neighbors
graph_read.handle_neighbors(depth>1) depends on → ppr_bfs.node_index_for accessor
migration tests depend on → store_queries (indexes created in create_tables_if_needed)
```

Integration harness tests (Python) exercise the full chain from wire → tools_dispatch →
graph_read → store_queries, confirming all cross-component boundaries are wired correctly.

---

## Integration Harness Plan

### Suite Selection for vnc-018

vnc-018 adds a new server tool, touches schema/storage, and modifies store query paths.
Based on the suite selection table:

| Feature area | Required suites |
|-------------|----------------|
| New server tool (context_graph) | `tools`, `protocol` |
| Schema migration (v26→v27) | `lifecycle` (restart persistence), `volume` |
| New store query functions | `tools`, `lifecycle`, `edge_cases` |
| Any change | `smoke` (minimum gate) |

**Suites to run**: `smoke` (gate), `protocol`, `tools`, `lifecycle`, `edge_cases`

### Existing Suite Coverage

| Suite | What it covers for vnc-018 |
|-------|---------------------------|
| `protocol` | P-03 tool count (14 context_* tools after AC-16 update) |
| `tools` | After new tests added: all three modes exercised (AC-20) |
| `lifecycle` | Restart persistence unaffected; correction chains still work |
| `edge_cases` | Unicode content in chain mode; boundary IDs |
| `smoke` | Minimum gate — existing smoke tests validate base server health |

### New Python Integration Tests Required

All new tests belong in `product/test/infra-001/suites/` using the `server` fixture
(function scope, fresh DB) unless noted.

#### 1. test_protocol.py — P-03 tool count update (AC-16 / R-14)

```python
# MODIFY existing test_list_tools_returns_thirteen → test_list_tools_returns_fourteen
# Assert len(tools) == 14
# Assert "context_graph" in [t.name for t in tools]
```

#### 2. test_tools.py — AC-20 core mode coverage

```python
def test_graph_chain_basic(server):
    # Store 3 entries; correct A→B, B→C; call chain on B; assert A,B,C returned

def test_graph_current_resolves_deprecated(server):
    # Store A; correct A→B; call current on A; assert B returned

def test_graph_neighbors_outgoing_depth1(server):
    # Store X, Y; write edge X→Y (Prerequisite); call neighbors id=X depth=1; assert Y in edges

# These three cover AC-20 — all three modes exercised.
```

#### 3. test_tools.py — AC-05a / R-21 asymmetry pair (Critical)

```python
def test_graph_current_nonexistent_id_returns_error(server):
    # Call current with id=999999; assert error response (not empty result)
    # COMMENT: "current mode returns error for non-existent ID — intentional asymmetry with chain mode (AC-05a, R-21)"

def test_graph_chain_nonexistent_id_returns_empty(server):
    # Call chain with id=999999; assert empty entries list, no error
    # COMMENT: "chain mode returns empty for non-existent ID — intentionally asymmetric with current (AC-04, R-21)"
    # These two tests must exist as a matched pair. See IMPLEMENTATION-BRIEF.md R-21.
```

#### 4. test_tools.py — R-20 orphaned deprecated terminal (Critical)

```python
def test_graph_current_orphaned_deprecated_returns_error(server):
    # Store entry D; call context_deprecate on D (no superseded_by set)
    # Call current with id=D; assert error "no active terminal found"
    # Assert D itself is not returned as entry
    # COMMENT: "Orphaned deprecated entry (superseded_by IS NULL, status=Deprecated)
    #          is NOT a valid terminal. Tests the AND e.status='Active' CTE filter. (R-20)"
```

#### 5. test_tools.py — R-03 staleness (Critical — documents expected behavior)

```python
def test_graph_neighbors_depth1_sees_fresh_write(server):
    # Store X, Y; write edge X→Y; call neighbors depth=1; assert Y in edges

@pytest.mark.xfail(strict=False, reason="Expected: depth>1 BFS uses pre-tick graph, edge not visible yet — R-03")
def test_graph_neighbors_depth2_does_not_see_fresh_write(server):
    # Store X, Y, Z; write edge X→Y, Y→Z; immediately call neighbors id=X depth=2
    # Assert Z NOT in edges (pre-tick graph has no edges yet)
    # COMMENT: "This asserts expected staleness, not a bug. Do not 'fix' by adding
    #          a tick wait — the point is that depth>1 lags by one tick interval. (R-03, ADR-005)"
```

Note: The xfail marker on the staleness test documents expected behavior. The `strict=False`
allows it to pass if the test environment happens to have a pre-loaded graph state.

#### 6. test_tools.py — EdgeRecord.metadata null (R-15)

```python
def test_graph_neighbors_edgerecord_metadata_is_null(server):
    # Store X, Y; write edge; call neighbors; inspect raw response JSON
    # Assert "metadata" key present in each EdgeRecord with value null (not absent)
```

#### 7. test_tools.py — AC-10a Supersedes silent exclusion no warning field (R-06)

```python
def test_graph_neighbors_supersedes_silently_excluded_no_warning_field(server):
    # Store X, Y, Z; write Supports X→Y and Supersedes X→Z
    # Call neighbors id=X edge_types=[] direction=both
    # Assert Y in edges, Z not in edges
    # Assert "excluded_types" not in response JSON, "warnings" not in response JSON
```

### Fixture Usage

All new tests use `server` (function scope, fresh DB). No `shared_server` or
`populated_server` needed for these scenarios — each test sets up its own state.

---

## Non-Negotiable Test Requirements

The following tests must be present before Gate 3c. A missing test is a gate failure:

1. **AC-16** — `test_protocol.py` P-03 asserts 14 tools
2. **AC-19** — `migration_v26_to_v27.rs` asserts all 4 index names in sqlite_master
3. **AC-03b** — Raw JSON wire shape of `truncated` field (not just deserialized)
4. **R-03 staleness** — depth=2 immediate-write returns absent edge (expected behavior)
5. **R-20** — Orphaned deprecated terminal returns "no active terminal found" error
6. **AC-05a / R-21 asymmetry pair** — Both AC-04 (chain empty) and AC-05a (current error) on same non-existent ID, with comment stating asymmetry is intentional design

---

## Coverage Completeness Self-Check

- All 20 ACs (AC-01 through AC-20) are mapped to at least one test
- All 21 risks (R-01 through R-21) have at least one scenario
- All 7 schema cascade touch points are covered by migration and parity tests
- Integration harness tests cover all three modes (AC-20 requirement)
- Both forward-compat fields and Supersedes exclusion paths are tested independently
