# Gate 3a Report: vnc-035

> Gate: 3a (Component Design Review)
> Date: 2026-06-12
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 4 components map to ARCHITECTURE.md decomposition; 8→8b→8b′→8c order honored in pseudocode + handler plan; ADR-001..005 reflected. |
| Specification coverage | PASS | FR-01..FR-12 each have corresponding pseudocode/test coverage; no scope additions. |
| Risk coverage | PASS | R-01..R-11 each map to ≥1 named test scenario; priorities reflected (Critical R-01 first). |
| Interface consistency | PASS | `query_outgoing_edges`, `CarrySummary`, `OutgoingEdgeRow`, `edges_carried` ack identical across OVERVIEW + component files + architecture Integration Surface. |
| Knowledge stewardship | PASS | Both design agents (architect, pseudocode) have `## Knowledge Stewardship` with Queried + Stored/declined entries. |
| **AC-07 mandatory test (by name)** | **PASS** | `test_carry_forward_continues_on_edge_copy_failure` present by name with 4 assertions + fault-injection-seam note. |

## Detailed Findings

### 1. Architecture alignment
**Status**: PASS
**Evidence**:
- Component boundaries match ARCHITECTURE.md "Component Breakdown" exactly: `query_outgoing_edges`/`OutgoingEdgeRow` (store), `run_carry_forward_loop`/`CarrySummary` (tools.rs sibling of `run_redirect_loop`), `context_correct` handler step 8b′ + ack, docs cleanup.
- Pipeline order `8 → 8b → 8b′ → 8c → 9 → 10` (ADR-001) is reproduced identically in pseudocode/OVERVIEW.md (lines 49-56), `run_carry_forward_loop.md`, and `context_correct_handler.md` Edit 1.
- ADR-002 single-SQL eligibility predicate `NOT IN ('Supersedes','CoAccess','Informs')` carried verbatim into `query_outgoing_edges.md` with the superset-vs-incoming rationale comment (ADR-002 §1).
- ADR-003 (own write loop, count `true` only), ADR-004 (created_at=now, no provenance marker), ADR-005 (Contradicts forward-counted/reverse-not, disjointness on no-self-loop) all reflected in `run_carry_forward_loop.md`.
- Technology choices consistent with ADRs; no contradictions found.

### 2. Specification coverage
**Status**: PASS
**Evidence**: Every FR has corresponding pseudocode + test:
- FR-01/02/03 (carry by default / attach to B / goal regression) → handler pseudocode + `test_correct_carries_eligible_outgoing_by_default`, `..._attach_to_new_id_not_original`, `..._goal_advances_vision_root_regression`.
- FR-04/09 (eligibility = agent-declared only / no ceiling) → `query_outgoing_edges` SQL + `test_query_outgoing_excludes_derived_classes`, `test_correct_no_ceiling_all_carry_above_50`.
- FR-05 (shed via new id) → `test_shed_carried_edge_against_new_id` + negative.
- FR-06 (Contradicts bidirectional) → ADR-005 pseudocode + `test_carry_contradicts_*`.
- FR-07 (warn-and-continue) → mandatory AC-07 test.
- FR-08 (additive-on-triple) → `test_correct_composition_{idempotent,additive,changed_target}` + omission-does-not-shed.
- FR-10 (edges_carried ack) → handler Edit 2 + `test_correct_edges_carried_*`.
- FR-11 (carried metadata) → `test_carried_edge_metadata_is_fresh_agent`.
- FR-12 (docs cleanup) → `docs_cleanup.md` + DC-01..DC-05.
No unrequested features observed — `docs_cleanup.md` explicitly scopes out PRODUCT-VISION sync and over-editing of false-positive doc hits.

### 3. Risk coverage
**Status**: PASS
**Evidence**: Test-plan OVERVIEW.md Risk→Test Mapping (lines 24-38) assigns named tests to all R-01..R-11. Priorities reflected: R-01 (Critical) leads with 3 tests incl. the mandatory; R-02/R-03/R-04 (High) each have multiple named tests; R-09 (Low) resolved at plan time with a verified codebase finding rather than a test. Integration and edge-case scenarios present (Contradicts convergence, self-loop defensive, >50 no-ceiling, tick-window depth-1-vs-BFS).

### 4. Interface consistency
**Status**: PASS
**Evidence**:
- `OutgoingEdgeRow { target_id, relation_type, created_at }` identical in OVERVIEW shared-types, `query_outgoing_edges.md`, and ARCHITECTURE Integration Surface.
- `CarrySummary { found, carried, failed }` consistent across OVERVIEW, `run_carry_forward_loop.md`, ADR-003.
- `query_outgoing_edges(&self, source_id: u64) -> Result<Vec<OutgoingEdgeRow>>` matches the architecture contract (mirror of `query_incoming_edges` on `source_id`).
- `run_carry_forward_loop(store, original_id, new_entry_id) -> CarrySummary` (by value, not Option) consistent and rationale documented (handler always observes found/failed).
- `edges_carried` ack contract (count only, omitted when zero, counts `true` inserts) consistent across handler pseudocode, ADR-003, FR-10, AC-11.
- Data flow between 8b → 8b′ → 8c coherent; the count-vs-order coupling (re-passed edge conflicts in 8b′) is stated identically everywhere.

### 5. Knowledge stewardship compliance
**Status**: PASS
**Evidence**:
- Architect report: `## Knowledge Stewardship` with `Queried:` (context_briefing/context_get on #4041, #4473, vnc-017/vnc-015 patterns) and `Stored:` (ADRs #4983-#4987) plus an explicit no-deprecation reason. Active-storage agent obligation met.
- Pseudocode report: `## Knowledge Stewardship` with `Queried:` (context_search → #4041, #4459, ADRs) and an explicit "No new patterns to store (read-only tier)" reason. Read-only agent obligation met.

### AC-07 — Mandatory test (CRITICAL CHECK)
**Status**: PASS
**Evidence**: `test-plan/run_carry_forward_loop.md` lines 26-57 specifies **`test_carry_forward_continues_on_edge_copy_failure`** BY NAME, flagged MANDATORY / verified-by-name-at-Gate-3b, with all four assertions:
1. Correction returns success (line 49-50).
2. New entry Active + original Deprecated (line 51-52).
3. Edges copied before the failure persist on B (line 53-54).
4. `CarrySummary.failed` incremented + `tracing::warn!` fired (line 55-57).
Fault-injection-seam note present (lines 37-43): extends the vnc-017 table-rename-to-view precedent (`tools.rs:10197`), with the explicit caveat that assertion 3 requires a mid-loop/Nth-write seam, and a brief constraint that the implementation MUST expose whichever seam makes assertion 3 observable. The test is additionally indexed by name in test-plan OVERVIEW.md (lines 28, 42-60) and ACCEPTANCE-MAP.md AC-07. Pseudocode `run_carry_forward_loop.md` §Fault-injection seam (lines 159-190) specifies the matching `carry_write_edge` `#[cfg(test)]` indirection. This is exactly the omission that FAILed Gate 3b in vnc-017 (#4473) — it is present and complete here.

## Cross-validated factual claim

The test plan's R-09 resolution ("`idx_graph_edges_source_id` exists at db.rs:969, migration.rs:367") was verified accurate — the index is created in both `db.rs:969` and `migration.rs:367`. O-1 is correctly resolved at plan time; latency-only, no functional test needed.

## Warnings (non-blocking)

- **WARN (advisory only):** ADR-003 / pseudocode leave `failed`-exactness as developer choice (impl (a) approximate-in-prod / exact-under-test, vs impl (b) exact wrapper). This is by design and the AC-07 assertion 4 (`failed >= 1`) holds under the test seam regardless. No rework needed; flagged so Gate 3b confirms the chosen impl keeps assertion 4 observable.
- **WARN (advisory only):** O-2 (module split read.rs vs read_outgoing.rs) deferred to developer on live line count — correctly non-blocking per NFR-05; Gate 3b should confirm no touched file exceeds 500 lines.

## Rework Required

None — gate passes.
