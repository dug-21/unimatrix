# Gate 3a Report: crt-058

> Gate: 3a (Design Review)
> Date: 2026-07-05
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | 4 components + unchanged tick backstop match ARCHITECTURE Component Breakdown; helper signature, RemovedEdge, step-6.5 insertion, LOCKED predicate all match Integration Surface |
| Specification coverage | PASS | FR-01…FR-09 and NFR-01…NFR-06 each have corresponding pseudocode; no scope additions |
| Risk coverage | PASS | R-01…R-11 all mapped to named behavioral tests in test-plan OVERVIEW |
| Interface consistency | PASS | Shared types (RemovedEdge, edges_removed: Option<u64>) consistent across OVERVIEW, per-component pseudocode, and test plans; formatter signature (edges_removed before format) pinned identically |
| Knowledge stewardship | PASS | Architect Stored #5458–5461; pseudocode/spec/tester Queried; risk strategy carries block with "nothing novel -- {reason}" |

Load-bearing items (spawn-prompt): all 5 satisfied — see Detailed Findings.

One non-blocking WARN: SPECIFICATION domain-model line 26 still references `rows_affected()` for `edges_removed`; pseudocode correctly uses `tuples.len()`. Cosmetic staleness in a source doc, not in the design under review.

## Detailed Findings

### Load-bearing item 1 — LOCKED predicate
**Status**: PASS
**Evidence**: `pseudocode/eager-delete-helper.md` renders exactly `DELETE FROM graph_edges WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2 RETURNING source_id, target_id, relation_type`, `pool = store.write_pool_server()`, `?2 = EDGE_SOURCE_AGENT` bound as a constant ("NOT user input"), with inline comment "LOCKED — never widen by relation_type; never add a runtime superseded_by clause." OVERVIEW and ADR-003 both forbid a runtime `superseded_by` clause; enforcement is the subset test, not SQL.

### Load-bearing item 2 — R-03 atomicity, count = tuples.len()
**Status**: PASS
**Evidence**: Helper pseudocode uses one `DELETE … RETURNING` + one `fetch_all`; "no delete-then-separate-SELECT window." OVERVIEW: "Count = `tuples.len()`, never `rows_affected()`." The same `Vec<RemovedEdge>` feeds both the count and the audit metadata (`emit_edge_cleanup_audit(entry_id, &tuples, …)`). Test plan `test_delete_returning_is_single_statement_capture` + `test_count_source_of_truth_is_tuples_len_not_rows_affected` assert this.

### Load-bearing item 3 — AC-10 subset test (both real functions + chokepoint-exclusion)
**Status**: PASS
**Evidence**: `test-plan/deprecate-handler.md` `test_deprecate_eager_subset_of_tick_and_exactly_agent_edges`: fixture A runs the real `delete_agent_edges_for_entry` → R; fixture B runs the real `run_orphaned_edge_compaction` → T; asserts `R ⊆ T` AND `R == exactly the two agent edges`; fixtures seeded from one shared helper with pre-deprecation identity assertion (R-02). Chokepoint-exclusion `test_correct_successor_bearing_entry_never_invokes_eager_helper` drives the real `context_correct` handler, asserts no `edge_cleanup` audit event and the inbound agent edge survives (R-01 closure). Predicate string pinned by `test_eager_predicate_string_pinned`.

### Load-bearing item 4 — non-fatal / audit-on-non-empty / Some(0) vs None
**Status**: PASS
**Evidence**: `deprecate-handler.md`: `Ok(tuples)` → audit only `IF NOT tuples.is_empty()`, `edges_removed = Some(tuples.len())`; `Err(e)` → `warn!(entry, error)` (NFR-05, not debug), `edges_removed = None`. `response-formatter.md` per-format table: `Some(0)` renders literal `0` in all three formats, `None` omits (Json key absent, not `null`). AC-05 vs AC-06 distinguished behaviorally in the formatter and handler test plans.

### Load-bearing item 5 — edges_removed threaded, None at non-delete sites
**Status**: PASS
**Evidence**: `response-formatter.md`: `format_status_change` and `format_deprecate_success` both gain `edges_removed: Option<u64>` (positioned before `format`); `format_quarantine_success` / `format_restore_success` hardcode `None` internally (arity preserved); the step-5 idempotent early-return passes `None` (`deprecate-handler.md` step 5). Backward-compat byte-identity tests cover quarantine/restore.

### Architecture alignment
**Status**: PASS
**Evidence**: pseudocode/OVERVIEW Components table maps 1:1 to ARCHITECTURE §Component Breakdown (eager-delete-helper `edge_write.rs`, deprecate-handler `tools.rs:1413` step 6.5, response-formatter `mutations.rs:16`, audit path `server.rs:650`, tick backstop UNCHANGED). Helper signature `async fn delete_agent_edges_for_entry(store: &Store, entry_id: u64) -> Result<Vec<RemovedEdge>, EdgeDeleteError>` matches Integration Surface. `emit_edge_cleanup_audit` is a thin wrapper over the architecture-named `audit_fire_and_forget` — elaboration, not divergence. Audit record shape (operation `context_deprecate.edge_cleanup`, target_ids `[entry_id]`, tuple-JSON metadata) matches architecture §Audit record shape.

### Specification coverage
**Status**: PASS
**Evidence**: FR-01 (both directions) → helper OR-predicate; FR-02 (machine untouched) → `source=?2` agent-only; FR-03 (inline count) → formatter; FR-04 (audit) → audit-emit; FR-05 (non-fatal) → handler match arm; FR-06 (synchronous) → awaited inline before format; FR-07 (idempotent no delete) → placement past step-5 guard; FR-08 (no new persistence) → reuses graph_edges/write_pool_server/existing indexes; FR-09 (subset) → ADR-003 test. NFRs addressed (single indexed statement, write pool, bounded latency, additive advisory, warn log discipline, enumeration-bound completeness). No unrequested features in pseudocode.

### Risk coverage
**Status**: PASS
**Evidence**: test-plan/OVERVIEW Risk-to-Test Mapping covers all 11 risks with named tests; R-01 (Critical) gets subset + chokepoint-exclusion + negative-mutation scenarios; R-03/R-06/R-11 (High) each mapped; integration + edge-case scenarios (self-loop, high-degree, shared-edge, zero-agent, concurrent-tick) present. Priority emphasis reflected — the subset test is called out as the keystone.

### Interface consistency
**Status**: PASS
**Evidence**: `RemovedEdge { source_id: u64, target_id: u64, relation_type: String }` (derives Serialize → audit JSON keys) defined once in OVERVIEW and reused in helper + audit-emit. `edges_removed: Option<u64>` ran-vs-failed semantics consistent across OVERVIEW, formatter, and handler. Formatter parameter position (before `format`) pinned identically in pseudocode and `test_edges_removed_param_position_before_format`. No contradictions between component files.

### Knowledge stewardship compliance
**Status**: PASS
**Evidence**: Architect report — `## Knowledge Stewardship` with `Queried:` (#3910, #4167, #3883, #4425) and `Stored:` entries #5458–5461. Pseudocode report — `Queried:` block, read-only tier reason. Spec report — `Queried:` block, "No novel pattern stored (read-only tier)". Test-plan report — `Queried:` block + "Stored: nothing novel -- {reason}". RISK-TEST-STRATEGY.md carries its own block: `Queried:` + "Stored: nothing novel — {reason}". All active-storage and read-only obligations met with reasons; no missing blocks.

## Rework Required

None (PASS). Optional cleanup, non-blocking: update SPECIFICATION.md domain-model line 26 to reference `tuples.len()` instead of `rows_affected()` to match the resolved single-source-of-truth decision already encoded in the pseudocode and load-bearing items.
