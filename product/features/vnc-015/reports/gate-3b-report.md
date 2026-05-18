# Gate 3b Report: vnc-015

> Gate: 3b (Code Review) — Iteration 2
> Date: 2026-05-15
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All 8 components implemented per pseudocode; data structures and algorithms match |
| Architecture compliance | PASS | ADR-001 through ADR-010 all respected; component boundaries intact |
| 10×4 ADR-007 compliance (R-01) | PASS | All 10 variants in enum body, as_str(), from_str(); RelatedTo in PPR + BFS; Advances/Motivates absent |
| R-02: redirect_graph_edge RAII transaction | PASS | pool.begin().await? used; all 4 SQL statements via &mut *txn; no raw BEGIN/COMMIT strings |
| R-04: Bidirectional Contradicts | PASS | Both (A,B) and (B,A) written in validate_and_write_edges; both deleted in delete_graph_edge; all 4 rows in redirect txn |
| R-10: default_rules() 2-argument callers | PASS | All callers pass (history, stale_edges); no single-argument calls remain |
| context_edge validation pipeline order | PASS | Steps 1–7: capability → source fetch → source status → self-ref → new_target_id check → edge type → target validation |
| No OwnershipViolation in context_edge | PASS | No OwnershipViolation error variant found anywhere in server code |
| CONSTRAINT 2: edge_write.rs ≤ 500 lines | PASS | 486 lines |
| CONSTRAINT 7: redirect RAII (no BEGIN/COMMIT strings) | PASS | pool.begin().await? RAII confirmed; no raw SQL transaction strings |
| CONSTRAINT 8: test asserting 13 tools (iter 2 fix) | PASS | test_list_tools_returns_thirteen present; docstring updated; context_edge in expected list |
| CONSTRAINT 9: all default_rules() callers updated | PASS | All call sites use 2-argument form |
| Compile check | PASS | cargo build --workspace — Finished dev profile with warnings only, no errors |
| No stubs or placeholders | PASS | No todo!(), unimplemented!(), TODO, FIXME found in new code |
| .unwrap() in non-test code (R-11) | WARN | tools.rs redirect arm: params.new_target_id.unwrap() — logically safe (Step 7 guard), but violates project rule; no blocking impact |
| Interface implementation | PASS | EdgeInput, EdgeParams, validate_and_write_edges, delete_graph_edge, redirect_graph_edge all match pseudocode and IMPLEMENTATION-BRIEF signatures |
| Test case alignment | PASS | Unit tests present for all component test plan scenarios; integration tests via read.rs and observe crate cover AC-11, AC-12, AC-13, AC-14; PPR/BFS tests cover AC-17 |
| Security | PASS | No hardcoded secrets; no path traversal in new code; input validated at all entry points; SQL uses parameterized queries throughout |
| Knowledge stewardship | PASS | All rust-dev agent reports contain ## Knowledge Stewardship sections with Queried: and Stored: entries |

---

## Detailed Findings

### Iteration 2 Delta

**Only the previously-failed check was re-validated for regressions. All prior PASSes held.**

#### CONSTRAINT 8 Fix Verification
**Status**: PASS

**Evidence** (`product/test/infra-001/suites/test_protocol.py` lines 36–57):
- Function renamed to `test_list_tools_returns_thirteen`
- Docstring updated to "P-03: tools/list returns exactly 13 context_* tools (vnc-015: +context_edge)"
- `"context_edge"` present in expected list at line 56

Both "thirteen" and "context_edge" confirmed present via grep; "twelve" no longer appears in the test function.

---

### Check 1: Pseudocode Fidelity
**Status**: PASS

**Evidence**: Each of the 8 components matches its pseudocode:
- `edge_write.rs` (486 lines): `EDGE_SOURCE_AGENT = "agent"`, `EdgeValidationError` variants, `validate_and_write_edges` Phase A/B split, `delete_graph_edge` idempotent with Contradicts bidirectional, `redirect_graph_edge` RAII txn
- `graph.rs` (unimatrix-engine): 10 new variants in enum body, as_str(), from_str() — all before wildcard arm per ADR-007
- `graph_ppr.rs`: RelatedTo in positive_out_degree_weight and personalized_pagerank; Advances/Motivates absent
- `graph_expand.rs`: RelatedTo in positive BFS set; Advances/Motivates absent
- `read.rs`: stale_dependency_edges SQL JOIN query added to compute_graph_cohesion_metrics; OR-clause bidirectional fix in query_contradicts_edges_for_entry
- `detection/scope.rs`: DependencyOnDeprecatedRule with constructor injection pattern; Severity::Warning; no I/O in detect()
- `detection/mod.rs`: 23rd rule registered; default_rules() signature changed
- `tools.rs`: EdgeInput, EdgeParams, StoreParams.edges, CorrectParams.edges, context_edge handler; Phase A pre-insert inline validation + Phase B validate_and_write_edges

---

### Check 2: Architecture Compliance
**Status**: PASS

**Evidence**: All ADRs respected:
- ADR-001: type resolution + target validation run before entry insert
- ADR-003: partial-write blast radius accepted; redirect is the only transactional exception
- ADR-004: DependencyOnDeprecatedRule receives Vec<(u64, u64)> via constructor; context_cycle_review pre-queries via query_stale_prerequisite_edges_for_cycle
- ADR-005: edge_write.rs extracted as pub(crate) module
- ADR-006: RelatedTo in PPR/BFS; Advances and Motivates absent (write-only until Phase 2)
- ADR-007: wildcard arm last in from_str(); all 10 new variants listed before it
- ADR-008: EDGE_SOURCE_AGENT = "agent" used in both source and created_by
- ADR-009: context_edge handler uses pool.begin().await? for redirect
- ADR-010: get_entry_by_id (via store.get()) for target validation

---

### Check 3: 10×4 ADR-007 Compliance (R-01 critical risk)
**Status**: PASS

**Evidence** (unimatrix-engine/src/graph.rs verified by grep):
- Enum body: All 10 variants present — Advances, Motivates, Cites, Asserts, Mentions, Refutes, Tests, DerivedFrom, About, RelatedTo
- as_str(): All 10 match arms present
- from_str(): All 10 arms present BEFORE wildcard `_ => None` (ADR-007 ordering)
- graph_ppr.rs: RelatedTo in positive sets; comment explicitly notes "Advances and Motivates intentionally absent"
- graph_expand.rs: RelatedTo in positive BFS loop at line 144; comment explicitly notes "Advances and Motivates intentionally absent"

---

### Check 4: R-02 — redirect_graph_edge RAII Transaction
**Status**: PASS

**Evidence** (edge_write.rs lines 310, 402):
- `pool.begin().await?` at line 310
- `txn.commit()` at line 402
- No raw `BEGIN`/`COMMIT`/`ROLLBACK` SQL strings found anywhere in edge_write.rs
- RAII rollback on drop confirmed by module doc comment at line 294

---

### Check 5: R-04 — Bidirectional Contradicts
**Status**: PASS

**Evidence**:
- `validate_and_write_edges` (edge_write.rs:211): Checks `rel_type == RelationType::Contradicts` and writes (target→source)
- `delete_graph_edge` (edge_write.rs:261): Checks `relation_type == "Contradicts"` and deletes reverse direction
- `redirect_graph_edge` (edge_write.rs:315-368): Contradicts branch handles all 4 rows (DELETE A→B, DELETE B→A, INSERT A→B', INSERT B'→A) within the same transaction
- `context_edge add mode` (tools.rs): After writing primary direction, checks Contradicts and writes reverse

---

### Check 6: R-10 — default_rules() Signature Change
**Status**: PASS

**Evidence** (all callers verified):
| Caller | 2-argument |
|--------|-----------|
| tools.rs context_cycle_review (line 2174) | `default_rules(history_slice, stale_edge_pairs)` |
| report.rs (line 653) | `detection::default_rules(None, vec![])` |
| recurring_friction.rs (line 41) | `detection::default_rules(None, vec![])` |
| detection/mod.rs tests (lines 234–377) | all use `default_rules(None, vec![...])` or `default_rules(Some(&mvs), vec![])` |

No single-argument calls remain.

---

### Check 7: context_edge Validation Pipeline Order
**Status**: PASS

**Evidence** (tools.rs lines 2925–3010):
1. Line 2925: `require_cap(Capability::Write)` — capability gate
2. Line 2928: `entry_store.get(params.source_id)` — source fetch
3. Lines 2937–2938: `Status::Quarantined || Status::Deprecated` check — source frozen gate
4. Line 2952: `params.source_id == params.target_id` — self-ref check
5. Line 2982: `new_target_id.is_some()` rejected for add/remove — presence check
6. Line 2992: `RelationType::from_str()` — edge type resolution
7. Line 3010: `validate_target(target_id)` — target validation

Matches the 7-step pipeline approved at Gate 3a.

---

### Check 8: No OwnershipViolation
**Status**: PASS

**Evidence**: grep for "OwnershipViolation" across all of `crates/unimatrix-server/src/` returns zero results. Security gate is `Capability::Write` plus source entry status only, as specified.

---

### Check 9: CONSTRAINT 2 — edge_write.rs ≤ 500 lines
**Status**: PASS

**Evidence**: `wc -l edge_write.rs` returns 486 lines. Within the 500-line limit.

---

### Check 10: CONSTRAINT 8 — Tool Count Test Updated to 13
**Status**: PASS (fixed in iteration 2)

**Evidence** (`product/test/infra-001/suites/test_protocol.py`):
- Line 36: `def test_list_tools_returns_thirteen(server):`
- Line 37: docstring reads "P-03: tools/list returns exactly 13 context_* tools (vnc-015: +context_edge)"
- Line 56: `"context_edge"` present in expected list

---

### Check 11: .unwrap() in Non-Test Code (WARN)
**Status**: WARN

**Evidence**: `crates/unimatrix-server/src/mcp/tools.rs` redirect arm:
```rust
let new_target = params.new_target_id.unwrap();
```

Logically safe — Step 7 `ok_or_else()?` already returns an error if `new_target_id` is `None` for redirect mode. The unwrap cannot panic. However, project rule states "No `.unwrap()` in non-test code". A comment acknowledges the invariant. This is a minor quality gap; it does not block PASS per gate rules (WARNs acceptable).

---

### Check 12: Knowledge Stewardship
**Status**: PASS

**Evidence**: All rust-dev agent reports contain proper stewardship blocks with Queried: and Stored: entries covering entries #4431–#4436.

---

## Knowledge Stewardship

- Stored: entry #4437 "Adding a new MCP tool requires updating test_protocol.py tool count assertion" via /uni-store-lesson (iteration 1); nothing novel to store in iteration 2 — fix confirmed, pattern already stored.
