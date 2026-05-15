# vnc-015 Test Plan Overview: Typed Edge Write Path + context_edge Tool

## Test Strategy

This feature spans 4 crates and 8 components. Testing is structured in three tiers:

1. **Unit tests** — inline in each crate, covering function-level contracts, enum round-trips,
   detection rule logic, and SQL correctness assertions.
2. **Integration tests (infra-001)** — exercises the full MCP JSON-RPC path through the
   compiled binary for end-to-end acceptance criteria verification.
3. **Grep verification gates** — structural code correctness checks that cannot be unit-tested
   (SR-01 10×4 variant×site checklist, ADR-007 mitigation).

---

## Risk-to-Test Mapping

| Risk ID | Severity | Primary Test File | Test Count |
|---------|----------|-------------------|------------|
| R-01 | Critical | relation-type.md | 20 (10 round-trip + 10 Pass 2b survival) |
| R-02 | Critical | edge-write.md | 4 (RAII code review + 3 redirect integration) |
| R-03 | Critical | edge-write.md | 2 (idempotent re-assert: non-Contradicts + Contradicts) |
| R-04 | Critical | edge-write.md + context-edge-handler.md | 4 (Contradicts both directions per surface) |
| R-05 | Critical | context-edge-handler.md | 3 (rollback-on-failure, quarantined target, atomicity) |
| R-06 | High | context-edge-handler.md | 3 (quarantined source, deprecated source, active baseline) |
| R-07 | High | contradicts-fix.md | 4 (unidirectional compat, bidirectional, both directions) |
| R-08 | High | edge-input-params.md | 2 (first-error-abort, mixed valid/invalid slice) |
| R-09 | High | edge-input-params.md + context-edge-handler.md | 2 (context_store post-insert, context_edge pre-check) |
| R-10 | High | detection-rule.md | 3 (count=23, rule fires, compile gate) |
| R-11 | Medium | ppr-expand.md | 2 (RelatedTo flows, Advances/Motivates do not flow) |
| R-12 | Medium | edge-input-params.md | 1 (duplicate guard before edge writes) |
| R-13 | Medium | context-edge-handler.md | 1 (new_target_id rejection for add/remove) |
| R-14 | Medium | stale-dependency.md | 2 (SQL status=1 correct, status=0 not counted) |
| R-15 | Low | edge-write.md | 1 (EDGE_SOURCE_AGENT constant used, not magic string) |

---

## Acceptance Criteria Coverage Map

| AC-ID | Primary Test File | Secondary |
|-------|-------------------|-----------|
| AC-01 | edge-input-params.md | — |
| AC-02 | edge-input-params.md | — |
| AC-03 | relation-type.md | — |
| AC-04 | relation-type.md | ppr-expand.md |
| AC-05 | edge-write.md | — |
| AC-06 | edge-write.md | context-edge-handler.md |
| AC-07 | edge-input-params.md | context-edge-handler.md |
| AC-08 | edge-input-params.md | context-edge-handler.md |
| AC-09 | edge-input-params.md | — |
| AC-10 | edge-write.md | — |
| AC-11 | stale-dependency.md | — |
| AC-12 | detection-rule.md | — |
| AC-13 | detection-rule.md | — |
| AC-14 | relation-type.md | — |
| AC-15 | context-edge-handler.md | edge-input-params.md |
| AC-16 | contradicts-fix.md | — |
| AC-17 | ppr-expand.md | — |
| AC-18 | edge-write.md | — |
| AC-19 | context-edge-handler.md | — |
| AC-20 | context-edge-handler.md | — |
| AC-21 | context-edge-handler.md | — |
| AC-22 | context-edge-handler.md | — |
| AC-23 | context-edge-handler.md | — |
| AC-24 | context-edge-handler.md | — |
| AC-25 | context-edge-handler.md | edge-write.md |
| AC-26 | context-edge-handler.md | edge-write.md |

---

## Cross-Component Test Dependencies

The following components have integration dependencies that require coordinated test ordering:

1. **relation-type.md → edge-write.md**: `from_str()` correctness is prerequisite for all
   write path tests. Pass 2b survival tests in relation-type.md must use the same DB fixtures
   as edge-write.md integration tests.

2. **edge-write.md → context-edge-handler.md**: `validate_and_write_edges`, `delete_graph_edge`,
   and `redirect_graph_edge` are `pub(crate)` — they can only be tested via the handler. All
   edge-write integration coverage flows through the MCP handler.

3. **stale-dependency.md → detection-rule.md**: `stale_dependency_edges` SQL and the
   `DependencyOnDeprecatedRule` both depend on a Prerequisite edge from a deprecated source.
   Shared DB fixture strategy applies.

4. **contradicts-fix.md → edge-write.md**: The bidirectional query fix is meaningless without
   bidirectional writes in place. Test order: write Contradicts (edge-write), then query
   (contradicts-fix).

5. **detection-rule.md → default_rules() callers**: The signature change is a compile-time
   dependency. All callers must be updated before any test in detection-rule.md can run.

---

## Integration Harness Plan (infra-001)

### Suite Selection

vnc-015 touches server tool logic, store behavior, and security (capability enforcement).
Based on the suite selection table:

| Suite | Justification |
|-------|---------------|
| `tools` | New 13th tool (`context_edge`); `context_store`/`context_correct` parameter extensions |
| `protocol` | Tool count changes from 12 to 13; protocol suite asserts tool discovery |
| `lifecycle` | Multi-step flows: store-with-edges→search, edge redirect chains, correction with edges |
| `security` | Capability enforcement for `context_edge`; SourceFrozen validation |
| `edge_cases` | Empty edges vec, boundary mode handling, idempotent remove of non-existent edge |
| `contradiction` | Bidirectional Contradicts write/read/remove; query fix |
| `smoke` | Mandatory minimum gate |

Suites NOT required: `confidence` (no scoring change), `volume` (no schema migration),
`adaptation` (no allowlist/format change).

### Existing Suite Tests Covering This Feature

- `tools` suite: existing tool count test must be updated 12 → 13 (OQ-4 from ARCHITECTURE.md).
  Locate the specific test in `suites/test_tools.py` during Stage 3b.
- `security` suite: existing `Capability::Write` enforcement tests implicitly cover
  `context_store`/`context_correct`. `context_edge` needs new explicit capability test.
- `contradiction` suite: existing bidirectional Contradicts tests may reveal pre-existing
  asymmetry. Run before implementation to establish baseline.

### New Integration Tests Required

The following new tests must be added to `suites/test_tools.py` and `suites/test_lifecycle.py`:

#### test_tools.py — context_edge tool

```python
# Tool registration
def test_context_edge_tool_registered(server):
    # Assert 13 tools; assert context_edge present with correct parameter schema

# Add mode
def test_context_edge_add_basic(server):
    # Store two entries; add Supports edge; query GRAPH_EDGES; assert row present

def test_context_edge_add_contradicts_bidirectional(server):
    # Add Contradicts edge; assert both (A,B) and (B,A) rows present

def test_context_edge_add_idempotent(server):
    # Add same edge twice; assert 1 row (INSERT OR IGNORE); assert no error

def test_context_edge_add_target_not_found(server):
    # Add edge to non-existent target_id; assert TargetNotFound error

def test_context_edge_add_quarantined_target(admin_server):
    # Quarantine target; add edge; assert TargetQuarantined error

def test_context_edge_add_deprecated_target_succeeds(server):
    # Deprecate target; add edge; assert success and row present

def test_context_edge_add_source_frozen_quarantined(admin_server):
    # Quarantine source; call context_edge add; assert SourceFrozen

def test_context_edge_add_source_frozen_deprecated(server):
    # Deprecate source; call context_edge add; assert SourceFrozen

def test_context_edge_add_self_referential(server):
    # source_id == target_id; assert SelfReferential error

def test_context_edge_add_unknown_edge_type(server):
    # edge_type="InvalidType"; assert UnknownType error

def test_context_edge_add_requires_write_capability(server):
    # Agent without Capability::Write; assert permission error

def test_context_edge_add_no_ownership_check(server):
    # Agent B operates on Agent A's entry; assert success (AC-22)

def test_context_edge_add_new_target_id_rejected(server):
    # mode="add" with new_target_id present; assert error

# Remove mode
def test_context_edge_remove_basic(server):
    # Add edge; remove it; assert row gone; assert idempotent on second remove

def test_context_edge_remove_contradicts_both_directions(server):
    # Add Contradicts; remove; assert both direction rows deleted

def test_context_edge_remove_non_existent_idempotent(server):
    # Remove non-existent edge; assert success (no EdgeNotFound)

def test_context_edge_remove_new_target_id_rejected(server):
    # mode="remove" with new_target_id present; assert error

# Redirect mode
def test_context_edge_redirect_basic(server):
    # Add A→B; redirect to B'; assert A→B gone AND A→B' present

def test_context_edge_redirect_contradicts_atomic(server):
    # Add A↔B; redirect to B'; assert all 4 rows updated

def test_context_edge_redirect_rollback_on_bad_new_target(server):
    # Redirect to non-existent new_target_id; assert TargetNotFound;
    # assert original A→B still present (rollback confirmed)

def test_context_edge_redirect_new_target_id_required(server):
    # mode="redirect" without new_target_id; assert error

def test_context_edge_no_side_effects(server):
    # Confirm no embedding, confidence, or duplicate detection triggered (AC-20)
```

#### test_tools.py — context_store / context_correct edges param

```python
def test_store_with_edges_backward_compatible(server):
    # Call context_store without edges; behavior identical to baseline (AC-01)

def test_store_with_edges_writes_graph_rows(server):
    # Store entry with edges=[{Supports, target}]; assert GRAPH_EDGES row (AC-05)

def test_store_with_edges_contradicts_bidirectional(server):
    # Store with Contradicts edge; assert both directions in GRAPH_EDGES (AC-06)

def test_store_with_edges_target_not_found_fails_all(server):
    # Non-existent target; assert call fails; assert no entry written (AC-07)

def test_store_with_edges_quarantined_target_fails_all(admin_server):
    # Quarantined target; assert call fails; assert no entry written (AC-07)

def test_store_with_edges_deprecated_target_succeeds(server):
    # Deprecated target; assert success; assert edge row written (AC-07)

def test_store_with_edges_duplicate_skips_edges(server):
    # Duplicate content with edges; assert duplicate response; assert no new GRAPH_EDGES rows (AC-09)

def test_store_with_edges_idempotent_reassertion(server):
    # Same edge twice in two calls; assert exactly 1 row (AC-10)

def test_correct_with_edges_attaches_to_new_entry(server):
    # Correct entry with edges; assert GRAPH_EDGES row references new entry id, not deprecated (AC-02)

def test_store_with_edges_source_agent_attribution(server):
    # Store with edge; query GRAPH_EDGES.source; assert == "agent" (AC-05, AC-18)
```

#### test_lifecycle.py — edge lifecycle flows

```python
def test_edge_survives_server_restart(shared_server):
    # Store with edge; restart server; assert GRAPH_EDGES row still present

def test_contradicts_edge_suppression_works_after_bidirectional_write(server):
    # Write Contradicts edge; run context_search; assert suppression applies

def test_stale_dependency_appears_in_context_status(server):
    # Write Prerequisite edge; deprecate source; call context_status;
    # assert stale_dependency_edges >= 1 (AC-11)
```

### Fixture Selection for New Tests

| Test Group | Fixture |
|------------|---------|
| Basic add/remove/redirect | `server` |
| Quarantine operations | `admin_server` |
| Cross-agent operations (AC-22) | `server` (enroll two agents) |
| Server restart persistence | `shared_server` |
| context_status stale count | `server` |

---

## SR-01 Grep Verification Procedure (ADR-007, Gate-3a)

For each of the 10 new variants, confirm presence in:
1. `crates/unimatrix-engine/src/graph.rs` — enum body: `Variant,`
2. `crates/unimatrix-engine/src/graph.rs` — `as_str()` match: `Self::Variant => "Variant"`
3. `crates/unimatrix-engine/src/graph.rs` — `from_str()` match: `"Variant" => Some(Self::Variant)`
4. `crates/unimatrix-engine/src/graph_ppr.rs` — positive set: **`RelatedTo` only** (REQUIRED)
5. `crates/unimatrix-engine/src/graph_expand.rs` — positive BFS set: **`RelatedTo` only** (REQUIRED)

Negative checks (must NOT appear in PPR or graph_expand):
- `Advances` — must NOT be in positive set of graph_ppr.rs or graph_expand.rs
- `Motivates` — must NOT be in positive set of graph_ppr.rs or graph_expand.rs

These grep checks are part of Gate-3a review. Stage 3c must re-verify all 10×4 cells
as part of the RISK-COVERAGE-REPORT.md (R-01 coverage evidence).

---

## Scope Boundaries

**In scope for test plans**: all behaviors from IMPLEMENTATION-BRIEF.md Component Map.

**Not tested in this feature**:
- `context_graph` traversal tool (Phase 2)
- PPR/graph_expand for `Advances`/`Motivates`/other write-only variants (Phase 2)
- `context_batch_write` (out of scope)
- Edge confidence floor (dropped)
- Per-edge agent attribution beyond `EDGE_SOURCE_AGENT` constant
