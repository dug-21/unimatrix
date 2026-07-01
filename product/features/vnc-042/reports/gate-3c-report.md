# Gate 3c Report: vnc-042

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-07-01
> Validated against: committed HEAD of `feature/vnc-042` (448b565c)
> Result: **PASS**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | R-01..R-08 gated risks Full coverage; each mapped to a passing test, independently re-run |
| 2. Test coverage completeness | PASS | TS-01..TS-09 + dead-end sub-cases + orthogonality matrix exercised; integration counts present |
| 3. Specification compliance | PASS | FR-01..FR-14 implemented+tested; NFR-05 canary green; NFR-06 additivity green; NFR-07 single-tool blast radius confirmed; AC-01..AC-08 verified |
| 4. Architecture compliance | PASS | `resolve_effective_id` reuses canonical `follow_to_current`; effective_id threads to fetch + edges; ResolutionNote handler-side per ADR-003; visibility widen correct |
| 5. Knowledge stewardship | PASS | RISK-COVERAGE-REPORT has stewardship block: Queried + Stored("nothing novel -- {reason}") |
| INT: smoke gate | PASS | Re-run: 26 passed / 0 failed |
| INT: feature suites | PASS | 6 new vnc-042 integration tests green; feature-relevant suites 0 failed |
| INT: xfail hygiene | PASS | No xfail markers added in branch → no GH Issue owed |
| INT: no tests deleted | PASS | No test defs/asserts removed; only additions + 1 precondition read migrated |
| INT: SR-02 migration scrutiny | PASS | `follow_supersessions=False` on precondition read only; no assertion weakened; no feature bug masked |

## Detailed Findings

### 1. Risk mitigation proof
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md maps R-01..R-12 to named tests. Independently re-ran the crux tests rather than trusting the report:
- R-01 (byte-identity): `test_none_json_byte_identical_to_base_object` green; `test_with_note_stripped_equals_base_formatter` green.
- R-02 (serde-default footgun, Critical): behavioral `test_get_handler_field_absent_resolves_to_terminal` + integration `test_get_default_resolves_deprecated_to_terminal` (field omitted through MCP JSON-RPC) green.
- R-03 (edge keying): unit `test_get_handler_resolved_edges_keyed_on_terminal` + integration `test_get_resolved_edges_keyed_on_terminal` (asserts terminal edges surfaced, requested-id edges NOT) green.
- R-04 (dead-end fail-loud): orphaned / quarantined / >50-hop / cycle / store-error unit suite + integration `test_get_deadend_returns_requested_id_loud_flag` green.
- R-07 (json resolution key): clean-passthrough-no-key + 3 non-clean presence tests green.
- R-08 (null successor footer): `test_note_asstored_null_successor_wellformed_footer` (no panic, no `#null`) green.
- R-09/R-10/R-11/R-12: accepted/deferred, correctly documented as not gated.

### 2. Test coverage completeness
**Status**: PASS
**Evidence**: 38 vnc-042 unit tests pass (19 handler/params, 13 formatter, plus canaries). All 6 new integration tests (OVERVIEW §4.3) present and green. >50-hop cap exercised through `context_get` (not only `graph_queries_tests.rs`). Orthogonality matrix (format × include_edges) exercised end-to-end. RISK-COVERAGE-REPORT §Integration includes per-suite counts and the 26-test smoke total.

### 3. Specification compliance
**Status**: PASS
**Evidence**: AC-01..AC-08 all verified with cited unit + integration evidence. NFR-05 byte-identity canary green unchanged (notice attaches in `format_single_entry_with_note`, never `format_single_entry`). NFR-06 `test_get_params_no_existing_field_removed_or_retyped` green. NFR-07 confirmed by diff: change confined to context_get surface, `resolve_supersessions` untouched, no schema/SQL. Tool description (FR-13/BLD-04) documents the default + `follow_supersessions=false` escape hatch.

### 4. Architecture compliance
**Status**: PASS
**Evidence**: `resolve_effective_id` calls `crate::mcp::graph_read::follow_to_current` (canonical copy in `graph_read_neighbors.rs`, widened `pub(super)`→`pub(crate)` + re-exported from `graph_read.rs`); the `graph_read_supersession.rs:122` duplicate and `handle_current` are NOT called. `effective_id` threads to both `entry_store.get` and `build_edges_view` (single-fetch invariant, R-03). ResolutionNote finalized handler-side and routed to the `_with_note` formatter variant per ADR-003. Dead-end returns originally-requested id per ADR-002.

### 5. Knowledge stewardship
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md §Knowledge Stewardship has `Queried:` (context_briefing — #5058, #5389, #5388) and `Stored: nothing novel -- {reason}` with an explicit reason (blast-radius FLAG #5099 and store-layer partitioning #5383 already exist). Reason present → PASS, not WARN.

### Integration Test Validation (mandatory)
**Status**: PASS
- Smoke gate re-run: `26 passed, 608 deselected` / 0 failed.
- Feature-relevant vnc-042 integration tests re-run: 6 green.
- SR-02 blast-radius migration (`test_correct_leaves_supersedes_edges_unchanged`): the only change is `follow_supersessions=False` added to the S-provenance PRECONDITION read; the AC-10 Supersedes-edge-exclusion assertions are untouched. Re-ran green. Does not mask a feature bug — it correctly inspects S's own as-stored provenance under the new locked default.
- xfail: no `@pytest.mark.xfail` added in the branch → no GH Issue owed. The 8 pre-existing xfails / 1 xpass are unrelated to vnc-042 (accounted in the report).
- No integration tests deleted or commented out (diff is additive + the one precondition migration).

## Rework Required
None.

## Scope Concerns
None. Two carried flags are LOCKED/accepted, not blockers:
- R-09: behavioral coverage for non-code durable-id consumers is impossible by design (LOCKED product bet #843); tool-description proxy only.
- R-11: JS `GetParams` parity surfaces only in the JS CI matrix (incl. Windows) — budget one post-PR CI round-trip.
</content>
