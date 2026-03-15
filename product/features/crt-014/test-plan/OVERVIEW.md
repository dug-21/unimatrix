# Test Plan Overview: crt-014 — Topology-Aware Supersession

## Test Strategy

crt-014 introduces a new graph module (`graph.rs`) in `unimatrix-engine`, removes hardcoded penalty constants from `confidence.rs`, and upgrades the search pipeline in `search.rs` to use topology-derived penalties and multi-hop successor injection. Testing is structured in three tiers:

1. **Unit tests** — `graph.rs` (new functions, edge cases, behavioral ordering invariants, depth-cap enforcement). These tests work directly with `EntryRecord` slices and require no store.
2. **Unit tests (migration)** — `confidence.rs` (removal of 4 old constant-value tests; verify no residual references). `search.rs` T-SP tests updated to behavioral ordering assertions.
3. **Integration tests** — `infra-001` suites exercising the compiled `unimatrix-server` binary through MCP JSON-RPC, covering multi-hop injection (AC-13) and cycle fallback (AC-16).

### Testing Philosophy

- **Risk-driven**: Critical risks (R-01, R-03, R-04, R-05, R-06) each require dedicated test functions with distinct inputs. No branch shares a test.
- **Behavioral ordering, not constant values**: Post-migration, penalty tests assert relative ordering (orphan softer than clean replacement) rather than absolute values (DEPRECATED_PENALTY == 0.7).
- **Atomic commit requirement**: The 4 tests removed from `confidence.rs` and the behavioral ordering tests added to `graph.rs` must land in the same commit (R-05).
- **Cumulative infrastructure**: All new unit tests extend existing test modules inside their respective source files (`graph.rs` inline `#[cfg(test)]` block, `confidence.rs` existing test block). No isolated scaffolding.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component | Test Location | Scenarios |
|---------|----------|-----------|---------------|-----------|
| R-01 | Critical | graph.rs | `graph.rs` unit tests | All 5 priority branches exercised in isolation: orphan, dead-end, partial-supersession, depth-1, depth-N |
| R-02 | High | graph.rs | `graph.rs` unit tests | 2-node cycle, 3-node cycle, valid DAGs, self-referential, empty |
| R-03 | Critical | graph.rs | `graph.rs` unit tests | 3-hop chain (A→B→C), depth-1, no-active-terminal, superseded intermediate, absent node |
| R-04 | Critical | graph.rs | `graph.rs` unit tests | Explicit edge-direction inspection via `edges_directed(A, Outgoing)` |
| R-05 | Critical | confidence.rs + graph.rs | Both files, same commit | 4 old tests removed, 4+ ordering tests added in graph.rs |
| R-06 | Critical | search.rs | `test_lifecycle.py` (infra-001) | Multi-hop A→B→C injects C; single-hop regression A→B injects B |
| R-07 | High | graph.rs | `graph.rs` unit tests | Chain of 11 (cap hit → None); chain of 10 (boundary → Some); chain of 9 |
| R-08 | High | search.rs | `test_lifecycle.py` (infra-001) | Cycle injected, mixed Active+Deprecated data: only non-active entries penalized |
| R-09 | High | graph.rs | `graph.rs` unit tests | Dangling `supersedes` ref: assert `Ok(graph)`, no panic |
| R-10 | High | search.rs | Code review | Confirm `build_supersession_graph` called inside existing `spawn_blocking` |
| R-11 | High | search.rs + confidence.rs | Shell grep | `grep DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` in non-test code return zero |
| R-12 | High | graph.rs | `graph.rs` unit tests | Decay formula at depths 1, 2, 5, 10; clamp at both ends |
| R-13 | Low | Cargo.toml | `cargo build` | Build clean; no unexpected feature transitive deps |
| IR-01 | Medium | search.rs | infra-001 / unit | `QueryFilter::default()` returns all statuses; graph includes Deprecated nodes |
| IR-02 | Medium | search.rs | `graph.rs` unit + infra-001 | Unified guard `superseded_by.is_some() || status == Deprecated` covers both paths |
| IR-03 | Medium | search.rs | Code review | No `graph_penalty` call for Active non-superseded entries |
| IR-04 | Medium | Cargo.toml | `cargo build` | `thiserror` present in engine Cargo.toml before add |

---

## Cross-Component Dependencies

```
confidence.rs  ──removes──▶  DEPRECATED_PENALTY, SUPERSEDED_PENALTY
                                    │
                                    ▼
graph.rs       ──defines──▶  ORPHAN_PENALTY, CLEAN_REPLACEMENT_PENALTY,
                               PARTIAL_SUPERSESSION_PENALTY, DEAD_END_PENALTY,
                               FALLBACK_PENALTY, HOP_DECAY_FACTOR
                                    │
                                    ▼
search.rs      ──imports──▶  graph_penalty, find_terminal_active, FALLBACK_PENALTY
               ──removes──▶  DEPRECATED_PENALTY import (line 18)
               ──updates──▶  T-SP-01, T-SP-02, T-SP-04, T-SP-05, T-SP-06, T-SP-07,
                              T-SP-08 (remove constant references, add behavioral assertions)
```

Test order dependency: `graph.rs` unit tests must be green before `search.rs` integration tests can pass, because the search pipeline depends on the graph functions.

---

## Integration Harness Plan (infra-001)

### Feature Surface

crt-014 modifies:
- Search pipeline penalty marking (Step 6a) — topology-derived penalties
- Search pipeline successor injection (Step 6b) — multi-hop traversal
- No new MCP tools; no schema changes; no briefing changes

### Applicable Suites

| Suite | Applies? | Reason |
|-------|----------|--------|
| `smoke` | YES (mandatory) | Minimum gate — search smoke path must pass |
| `tools` | YES | `context_search` is a modified tool; penalty and injection changes are MCP-visible |
| `lifecycle` | YES | Multi-hop injection (AC-13) and cycle fallback (AC-16) are multi-step flows |
| `confidence` | NO | No changes to the confidence formula or weight factors |
| `contradiction` | NO | Contradiction detection is unchanged |
| `security` | NO | No new security surface; no scanner changes |
| `volume` | NO | Graph construction ≤5ms at 1,000 entries is a unit-level benchmark, not a volume suite concern |
| `edge_cases` | NO | Edge cases in graph.rs are covered by unit tests |

### Existing Suite Coverage Assessment

**`lifecycle` suite** (16 tests) covers correction chains and store→search flows. It does NOT currently include:
- Multi-hop successor injection (A→B→C injects C, not B) — **gap: AC-13**
- Cycle fallback search behavior — **gap: AC-16**
- `FALLBACK_PENALTY` applied only to deprecated/superseded entries when cycle detected — **gap: R-08**

**`tools` suite** (53 tests) covers `context_search` parameters and response formats. Existing tests use Active entries; tests that verify relative penalty ordering between deprecated and active entries exist but assert old constant values — **gap: AC-12 behavioral assertion**.

### New Integration Tests Required

Both new tests belong in `suites/test_lifecycle.py`. They require multi-step state setup (store → deprecate/supersede → search) which matches the lifecycle suite's fixture and fixture scope.

#### Test 1: `test_search_multihop_injects_terminal_active` (AC-13, R-06)

```python
@pytest.mark.smoke  # optional: yes, this is critical path
def test_search_multihop_injects_terminal_active(server):
    # Store A (will become superseded via correction chain)
    # Store B (will supersede A, will itself be superseded)
    # Store C (active terminal)
    # Build chain A→B→C via context_correct calls
    # Search for content matching A
    # Assert: C.id appears in results; B.id does NOT appear as the injected successor
```

Fixture: `server` (fresh DB per test — no state leakage across searches).

#### Test 2: `test_search_cycle_fallback_uses_flat_penalty` (AC-16, R-08)

This test cannot be implemented directly through the MCP interface without ability to create a cycle in the supersession graph. Cycles can only be created via direct store manipulation or via testing at unit/engine level. **Conclusion**: AC-16 cycle fallback is best verified at the unit level (search.rs unit test calling `build_supersession_graph` with cycle data directly) or via a store-level test that bypasses the MCP interface.

File a note in the test plan: the infra-001 harness cannot inject a supersession cycle through the MCP interface (no tool allows setting `supersedes` to create a cycle). AC-16 verification is unit-test-only.

#### Test 3: `test_search_deprecated_entry_visible_with_topology_penalty` (AC-12)

```python
def test_search_deprecated_entry_visible_with_topology_penalty(server):
    # Store entry A (active)
    # Store entry B (active, similar content)
    # Deprecate B
    # Search: B must appear with penalty applied (still visible in Flexible mode)
    # Assert: B.id present in results; A ranked above B
```

Fixture: `server`.

### Smoke Test Command (Mandatory Gate)

```bash
cd /workspaces/unimatrix-crt-014/product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60
```

### Selective Suite Commands

```bash
# Minimum: smoke gate
python -m pytest suites/ -v -m smoke --timeout=60

# Primary suites for crt-014
python -m pytest suites/test_lifecycle.py suites/test_tools.py -v --timeout=60

# Full suite (pre-merge)
python -m pytest suites/ -v --timeout=60
```

---

## Acceptance Criteria Coverage Map

| AC-ID | Component | Test Type | Test File |
|-------|-----------|-----------|-----------|
| AC-01 | Cargo.toml | Shell | `cargo build --workspace` |
| AC-02 | lib.rs | Shell | `cargo doc --package unimatrix-engine` |
| AC-03 | graph.rs | Unit | `graph.rs` inline tests |
| AC-04 | graph.rs | Unit | `graph.rs` inline tests |
| AC-05 | graph.rs | Unit | `graph.rs` inline tests |
| AC-06 | graph.rs | Unit | `graph.rs` inline tests |
| AC-07 | graph.rs | Unit | `graph.rs` inline tests |
| AC-08 | graph.rs | Unit | `graph.rs` inline tests |
| AC-09 | graph.rs | Unit | `graph.rs` inline tests |
| AC-10 | graph.rs | Unit | `graph.rs` inline tests |
| AC-11 | graph.rs | Unit | `graph.rs` inline tests |
| AC-12 | search.rs | Unit + infra-001 | `search.rs` + `test_lifecycle.py` |
| AC-13 | search.rs | infra-001 | `test_lifecycle.py::test_search_multihop_injects_terminal_active` |
| AC-14 | confidence.rs + search.rs | Grep | `grep -r DEPRECATED_PENALTY crates/` |
| AC-15 | confidence.rs + graph.rs | Test + Grep | `confidence.rs` test removal; `graph.rs` ordering tests |
| AC-16 | search.rs | Unit | `search.rs` inline test with cycle fixture data |
| AC-17 | graph.rs | Unit | `graph.rs` inline test |
| AC-18 | Workspace | Shell | `cargo build --workspace` |
