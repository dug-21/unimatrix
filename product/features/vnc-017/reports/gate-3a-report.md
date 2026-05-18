# Gate 3a Report: vnc-017

> Gate: 3a (Design Review)
> Date: 2026-05-18
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All three components match architecture decomposition and ADRs |
| Specification coverage | PASS | All 17 ACs and 9 NFRs covered; FR-07 table discrepancy resolved by ADR-003 reference in pseudocode |
| Risk coverage | PASS | All 14 risks mapped; Critical R-01/R-02/R-06 have explicit test scenarios |
| Interface consistency | PASS | Shared types match across OVERVIEW.md, component pseudocode, and architecture contracts |
| Knowledge stewardship | PASS | All four agent reports contain `## Knowledge Stewardship` sections with Queried entries |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:

- `query_incoming_edges` pseudocode: placed in `unimatrix-store/src/read.rs`, uses `read_pool()`, returns `Vec<IncomingEdgeRow>`, SQL excludes Supersedes at query level. Matches architecture §Component 1 exactly, including the C-07 comment requirement (documented in the doc comment block in query_incoming_edges.md).

- `redirect_loop` pseudocode: inserted at step 8c in `tools.rs`, after Phase B writes and before confidence recompute. Matches architecture §Execution Order within context_correct (steps 8c-1 through 8c-6). `REDIRECT_CEILING = 50` constant declared at module level per ADR-004. No `find_terminal_active` call (ADR-001 honored). No `TypedGraphState` read lock (NFR-05 honored).

- `response_format` pseudocode: post-call append to `CallToolResult` (preferred minimal-impact approach per architecture §Component 4). `format_correct_success` signature is unchanged. The `content.raw.text` mutation path acknowledges the rmcp API uncertainty with a documented fallback reconstruction pattern.

- Component interaction diagram in OVERVIEW.md matches the architecture's interaction diagram exactly (query_incoming_edges → redirect loop → format_correct_success with optional append).

- ADR decisions reflected: ADR-001 (new_entry.id direct), ADR-002 (SQL-level Supersedes exclusion), ADR-003 (warn+continue posture with explicit return contract table), ADR-004 (ceiling=50 with truncation response variant).

- Technology: no new crates introduced. sqlx 0.8 and tracing used via existing patterns.

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**:

All 13 functional requirements are addressed:

- FR-01 (query after commit, after Phase B): redirect_loop.md step 8c insertion point, after Phase B closing brace.
- FR-02 (new_entry.id is target, no traversal): OVERVIEW.md states `new_entry_id = correct_result.corrected_entry.id`; ADR-001 reference in pseudocode.
- FR-03 (query_incoming_edges in read.rs, returns Vec): query_incoming_edges.md function signature matches. **NOTE**: Spec FR-03 lists return type as `Vec<(u64, String, u64)>` (tuples) while architecture and pseudocode use `Vec<IncomingEdgeRow>` (named struct). Pseudocode follows the architecture's named struct — this is correct and the tuple form in FR-03 is the spec's informal description of the same data. No discrepancy in substance.
- FR-04 (Supersedes excluded at SQL level): SQL in query_incoming_edges.md shows `AND relation_type != 'Supersedes'` with required comment.
- FR-05 (redirect_graph_edge called per edge): redirect_loop.md loop body.
- FR-06 (source validation guard): redirect_loop.md per-edge loop matches Quarantined/Deprecated skip-with-warn pattern.
- FR-07 (return contract handling): pseudocode uses `Ok(()) → redirected++`, `Err(e) → warn, failed++` — matches ADR-003 contract table, NOT the spec's stale `Ok(true)/Ok(false)` table. This is the correct implementation reference per the risk strategy (R-01). The pseudocode does not perpetuate the FR-07 table error.
- FR-08 (no abort on redirect failures): redirect_loop.md handler never returns `Err` due to redirect failures.
- FR-09 (info summary log): `tracing::info!` with all counts after loop.
- FR-10 (4-variant response text): response_format.md covers all four conditions per the authoritative format table.
- FR-11 (existing response fields unchanged): format_correct_success called first; append is additive.
- FR-12 (context_edge unmodified): pseudocode makes no changes to edge_write.rs or context_edge handler.
- FR-13 (zero-edge path skips everything): `Ok(incoming) if incoming.is_empty() => None` branch.

All 9 non-functional requirements addressed:
- NFR-01 (inline, no spawn): no tokio::spawn in pseudocode.
- NFR-02 (no background workers): confirmed.
- NFR-03 (one transaction per edge): `redirect_graph_edge` called per edge; no shared transaction.
- NFR-04 (canonical accessor names + comment): C-07 comment documented.
- NFR-05 (no TypedGraphState lock): no reference to TypedGraphState in any pseudocode.
- NFR-06 (500-line rule for read.rs): architecture confirms ~30-line addition to 3,465-line file.
- NFR-07 (500-line handler limit): handler is ~145 + ~20 = ~165 lines.
- NFR-08 (no spanning transaction): confirmed by RAII-per-edge design.
- NFR-09 (no modification to listed functions): pseudocode explicitly states which files are modified and which are not.

No scope additions detected. The pseudocode does not implement any features beyond what the specification requires.

### Check 3: Risk Coverage

**Status**: PASS

**Evidence**:

All 14 risks from RISK-TEST-STRATEGY.md have test scenarios in the test plans:

- **R-01 (Critical)**: test-plan/redirect_loop.md "R-01 compile-time return contract structural test" — compile-time gate (Rust type system) plus behavioral test `test_redirect_loop_ok_unit_increments_redirected_not_failed`. The pseudocode itself correctly implements ADR-003 contract (not FR-07 table), so R-01 risk is mitigated by design.

- **R-02 (Critical)**: test-plan/query_incoming_edges.md `test_query_incoming_edges_excludes_supersedes_at_sql_level` — seeds two Supersedes rows, asserts empty Vec return. Structural proof of SQL-level exclusion.

- **R-03 (High)**: test-plan/query_incoming_edges.md `test_query_incoming_edges_high_cardinality_filters_correctly` — 1,000 noise rows + 3 target rows.

- **R-04 (High)**: test-plan/redirect_loop.md "10-edge call: assert store.get called exactly 10 times" — marked Partial coverage (mock pattern TBD). The agent report acknowledges this. Acceptable at design phase; implementation will choose mock or double.

- **R-05 (High)**: test-plan/redirect_loop.md `test_redirect_loop_ceiling_truncates_at_50_and_warns` — 55 edges, asserts 50 redirected, 5 remain, truncation warn emitted, response text contains "(truncated from 55, see logs)". Plus `test_redirect_loop_exactly_at_ceiling_no_truncation` for the boundary case.

- **R-06 (Critical)**: test-plan/redirect_loop.md covers: `test_redirect_loop_quarantined_source_skipped_not_failed`, `test_redirect_loop_deprecated_source_skipped_not_failed`, and critically `test_redirect_loop_mixed_status_redirects_valid_skips_invalid` — the mixed fan-in case that the risk-strategy identified as missing from the original ACs. The Contradicts bidirectional success path is also covered by the integration test `test_correct_auto_redirects_contradicts_edges`.

- **R-07 (High)**: test-plan/query_incoming_edges.md `test_query_incoming_edges_supersedes_only_returns_empty`. Response append behavior covered in redirect_loop tests.

- **R-08 (High)**: test-plan/OVERVIEW.md integration test `test_correct_redirected_edges_clear_dependency_detection` (AC-16). Test agent's OQ-3 flags a feasibility question about triggering the tick in infra-001 — acceptable open question for implementation phase.

- **R-09 (High)**: test-plan/redirect_loop.md `test_redirect_loop_unique_conflict_counts_as_success` — asserts redirected==1, failed==0 for UNIQUE conflict case.

- **R-10 (High)**: test-plan/redirect_loop.md `test_redirect_loop_phase_b_collision_no_duplicate_row`.

- **R-11 (High)**: test-plan/OVERVIEW.md integration test `test_correct_response_text_contains_redirect_summary` with exact substring assertion on CallToolResult.

- **R-12 (Med)**: test-plan/query_incoming_edges.md — Supersedes-only path returns empty under SQL-level exclusion (same test as R-07). The info log assertion is covered by AC-11 test in redirect_loop.md.

- **R-13 (Med)**: test-plan/OVERVIEW.md regression pass: existing context_edge(mode="redirect") tests run unchanged.

- **R-14 (Low)**: Accepted as untestable; code review gate documented in test-plan/redirect_loop.md.

Risk priorities are reflected in test plan emphasis: Critical risks (R-01, R-02, R-06) each have 2–4 test scenarios and are flagged as compile-time or behavioral structural tests.

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**:

- `IncomingEdgeRow` struct defined in `query_incoming_edges.md` (`source_id: u64, relation_type: String, created_at: u64`) matches OVERVIEW.md shared types section and architecture §Integration Surface exactly.

- `RedirectSummary` fields (found, skipped, redirected, failed, truncated, total_raw) defined in OVERVIEW.md and referenced consistently across redirect_loop.md (where accumulated) and response_format.md (where consumed). No field name mismatch.

- `REDIRECT_CEILING: usize = 50` defined in OVERVIEW.md and redirect_loop.md constant block — consistent.

- `query_incoming_edges` signature: `async fn(&self, target_id: u64) -> Result<Vec<IncomingEdgeRow>>` — consistent across OVERVIEW.md integration surface table, query_incoming_edges.md signature section, and architecture §Integration Surface.

- Data flow: OVERVIEW.md diagram shows `correct_result.corrected_entry.id → new_entry_id` passed to redirect loop, then `Option<RedirectSummary>` flows to response_format step. This matches the step-by-step pseudocode in both redirect_loop.md and response_format.md.

- `format_correct_success` is called with `(&correct_result.deprecated_original, &correct_result.corrected_entry, ctx.format)` — consistent with the existing function signature in the architecture's Integration Surface table.

- One minor discrepancy noted: architecture §Integration Surface lists `format_correct_success` as `fn(original: &EntryRecord, correction: &EntryRecord, format: ResponseFormat) -> CallToolResult`, which is "unchanged or lightly extended." The pseudocode uses the post-call append approach (not a signature extension) — this is explicitly the "preferred" approach per architecture §Component 4. Consistent.

- `redirect_graph_edge` call in redirect_loop.md: `(store, source_id, original_id, new_entry_id, relation_type, created_at)` — 6 parameters. Architecture §Integration Surface shows the same 6-parameter signature. Consistent.

- No contradictions found between component pseudocode files.

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

All four agent reports contain a `## Knowledge Stewardship` section:

- **vnc-017-agent-1-pseudocode**: `Queried:` entries for pattern search (found #4459, #4458, #4078) and ADR entries (#4463, #4460, #4461, #4462). `Stored: nothing novel to store` with reason (all patterns follow established read.rs and warn-and-continue patterns). Compliant.

- **vnc-017-agent-2-spec**: `Queried:` mcp__unimatrix__context_briefing, 9 entries. `Stored: nothing novel` with reason (feature-specific decisions). Compliant.

- **vnc-017-agent-2-testplan**: `Queried:` context_briefing (10 entries) + context_search (ADR entries + test pattern search). `Stored: nothing novel to store` with reason (standard patterns, defer R-02 pattern to Stage 3c). Compliant.

- **vnc-017-agent-3-risk**: `Queried:` four knowledge searches. `Stored: nothing novel to store` with reason (R-01 divergence is feature-specific). Compliant.

All active-storage-capable agents (spec, risk) have either Stored or Declined with reason. The pseudocode agent is a design agent that queries and follows patterns — its Queried entries are present. All compliant.

---

## Rework Required

None.

---

## Notes on Open Questions (Not Blockers)

**OQ-1 (rmcp Content mutation path)**: The pseudocode documents both the direct field mutation path (`content.raw.text`) and the reconstruction fallback. This is correctly deferred to compile time. Not a design gap.

**OQ-2 (store.get Err path as skipped++ vs failed++)**: The pseudocode uses skipped++ (conservative). This is consistent with the skip-with-warn posture — a source that cannot be validated is treated identically to an invalid source. Either choice is acceptable per architecture ADR-003; the test plan's AC-08 assertion targets the Quarantined/Deprecated status path, not the Err path specifically. The implementer may choose either counter without a gate rejection.

**AC-16 tick feasibility (test agent OQ-3)**: Whether the infra-001 harness can trigger a graph tick to assert DependencyOnDeprecated clearance is an implementation-phase question. The test is specified; harness feasibility is confirmed or adapted at Gate 3b. Not a design phase blocker.

**FR-07 table vs ADR-003**: The specification FR-07 table lists `Ok(true)/Ok(false)` (stale from an earlier draft). The pseudocode correctly ignores this and uses the ADR-003 return contract (`Ok(()) → redirected++`, `Err → warn+failed++`). The risk strategy (R-01) identifies this discrepancy. Since the pseudocode correctly follows ADR-003, R-01 is mitigated at design time. The spec FR-07 table remains formally incorrect but this does not block delivery since the pseudocode (the implementation reference) is correct.
