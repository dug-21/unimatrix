# Agent Report: crt-046-agent-2-spec

## Output

**File**: `product/features/crt-046/specification/SPECIFICATION.md`

**Functional Requirements**: 22 (FR-01 through FR-22)
**Acceptance Criteria**: 17 (AC-01 through AC-17)

## Key Decisions

1. **SR-04 resolved — INSERT OR IGNORE throughout**: The SCOPE.md body/Constraints contradiction (INSERT OR REPLACE for force=true vs INSERT OR IGNORE everywhere) is resolved as INSERT OR IGNORE throughout. This aligns with the scope's Resolved Decisions §6 and matches the graph_edges drain behavior. The constraint section of the specification is the authoritative ruling.

2. **SR-01 addressed — parse_failure_count in review result**: AC-13 requires the `parse_failure_count` field (or equivalent) on the `CycleReviewRecord` response — not server log only. FR-03 specifies the per-cycle counter is tracked and returned. This makes silent drops observable to callers.

3. **SR-09 addressed — recency cap in FRs and ACs**: FR-18 specifies the `ORDER BY created_at DESC LIMIT 100` constraint on the goal_clusters cosine scan. AC-11 provides a deterministic verification via a 101-row seed test where the oldest row is the best match and must not appear in results.

4. **SR-05 addressed — schema cascade enumerated**: AC-12 lists all 7 touchpoints from entry #3894 explicitly. AC-17 adds the grep-clean enforcement criterion. Both are Gate 3a checks.

5. **Step 8b always runs**: Confirmed in FR-09 and AC-15. The memoisation early-return must not bypass step 8b; the constraint (item 9) explicitly calls this out for the architect.

6. **Cap-hit surfacing**: Pair-cap warning goes to server log only (not CycleReviewRecord). SR-06 is resolved as server-log-only; only `parse_failure_count` (SR-01) is added to the review result.

## Open Questions for Architect

- **SR-03**: `write_graph_edge` rows_affected() return-contract table must be specified in pseudocode before implementation (Gate 3a rework risk documented in entry #4041).
- **SR-08**: Zero-remaining-slots behavior when semantic search fills all k=20 slots — silent suppression is spec'd as acceptable; architect should confirm.
- **SR-02**: Whether a `weight_behavioral` shadow column should be reserved now for future NLI-edge weight promotion.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 20 entries returned. Applied: #3894 (cascade checklist), #3312 (observation input JSON format). Background context from #3397, #3402, #3409.
- Entry #3894 retrieved in full via `context_get` and applied directly to AC-12, AC-17, and the schema cascade constraint.
- Entry #3312 retrieved in full — confirmed `"id": N` (unquoted integer) format for context_get input; applied to FR-02 and the observation parsing constraint.
