# Alignment Report: crt-001

> Reviewed: 2026-02-24 (Phase 2 redo -- post security review)
> Artifacts reviewed:
>   - product/features/crt-001/architecture/ARCHITECTURE.md
>   - product/features/crt-001/architecture/ADR-001 through ADR-007
>   - product/features/crt-001/specification/SPECIFICATION.md
>   - product/features/crt-001/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-001/SCOPE.md (11 decisions, 18 ACs)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly implements crt-001 from M4 roadmap |
| Milestone Fit | PASS | First feature of Milestone 4 (Learning & Drift), depends only on M1+M2 (complete) |
| Scope Gaps | PASS | All 7 goals and 18 ACs from SCOPE.md addressed in source documents |
| Scope Additions | WARN | Architecture adds EntryStore trait extension (record_access) not explicitly in vision's crt-001 description |
| Architecture Consistency | PASS | Follows established patterns; 7 ADRs cover all key decisions including vote correction and trust gating |
| Risk Completeness | PASS | 16 risks, 66 scenarios; covers migration, dedup, atomicity, vote correction, trust bypass, security |
| Vote Correction Coverage | PASS | SCOPE Decision #10, Architecture ADR-006, Spec FR-09/FR-20, Risk R-16 (5 scenarios) |
| Trust Gating Coverage | PASS | SCOPE Decision #11, Architecture ADR-007, Spec FR-11/FR-21, Risk R-17 (4 scenarios) |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | USAGE_LOG table dropped | SCOPE.md Decision #2: AUDIT_LOG + EntryRecord fields cover all downstream needs. Reduces table count from proposed 2 new to 1 new. Rationale documented in SCOPE Background Research. |
| Simplification | access_count reused instead of usage_count | SCOPE.md Decision #1: Existing field serves identical purpose. Avoids unnecessary migration. |
| Addition | EntryStore::record_access trait method | Architecture C4 adds a trait method not in the vision's crt-001 description. However, SCOPE.md Goal #7 explicitly requests this. Aligned with scope, minor addition vs vision. |
| Enhancement | Vote correction (last-vote-wins) | SCOPE.md Decision #10: Agents can change their vote within a session. Architecture ADR-006 ensures atomicity. Not in original vision but supports vision's "active learning" goal. |
| Enhancement | FEATURE_ENTRIES trust gating | SCOPE.md Decision #11: Only Internal+ agents write FEATURE_ENTRIES. Architecture ADR-007 documents the enforcement point. Consistent with vision's trust hierarchy. |

## Variances Requiring Approval

None. All findings are PASS or WARN (informational).

## Detailed Findings

### Vision Alignment

The product vision defines crt-001 as:
> "USAGE_LOG table -- every retrieval logged with (entry_id, timestamp, agent_role, feature_id, tool, helpful). FEATURE_ENTRIES multimap links features to entries used. Populate usage_count, helpful_count, last_used_at on EntryRecord. Security alignment: Enables write rate limiting per agent and behavioral baseline establishment for anomaly detection."

**Assessment**: The approved SCOPE.md deviates from the vision in several ways, all justified:

1. **USAGE_LOG dropped**: The vision specifies a USAGE_LOG table. The SCOPE's research determined that AUDIT_LOG already captures the per-retrieval event data (agent_id, tool, target_ids, timestamp) and that downstream features (crt-002, crt-004, col-002) need only EntryRecord aggregate fields + AUDIT_LOG. This is a simplification, not a reduction in capability. **PASS -- the data needs are met through different means.**

2. **Two-counter helpfulness**: The vision mentions `helpful` (singular). The SCOPE adds `unhelpful_count` based on gaming resistance research (Wilson score requires both positive and negative signals). This is an enhancement that supports the vision's goal of "active learning." **PASS -- improvement aligned with vision intent.**

3. **Session deduplication**: Not mentioned in the vision but required by the gaming resistance analysis. The vision's security alignment goal ("enables write rate limiting per agent and behavioral baseline establishment") implies gaming resistance. **PASS -- security enhancement supporting vision goals.**

4. **Vote correction (last-vote-wins)**: Not in the vision. Added based on the principle that a self-learning system should allow agents to correct early incorrect assessments. Prevents permanent quality degradation from speculative retrievals. **PASS -- enhances the "active learning" goal.**

5. **Trust-level gating on FEATURE_ENTRIES**: Not explicitly in the vision's crt-001 description, but consistent with the vision's trust hierarchy (Restricted = read-only). Prevents analytics pollution from auto-enrolled unknown agents. **PASS -- consistent with vision's security model.**

### Milestone Fit

crt-001 is the first feature of Milestone 4 (Learning & Drift / Cortical Phase). The milestone dependency graph shows:
```
M2: MCP Server (vnc) [COMPLETE]
 |-> M4: Learning & Drift (crt)
```

crt-001 depends only on M1 (store, vector, embed, core traits) and M2 (server, tools). Both are complete. No dependency on M3 (Agent Integration, deferred). **PASS.**

The feature stays within M4's scope: "Turn passive knowledge accumulation into active learning." It does not implement M4 features beyond crt-001 (no confidence computation from crt-002, no contradiction detection from crt-003, no co-access boosting from crt-004). **PASS -- no milestone bleeding.**

### Architecture Review

The architecture follows established Unimatrix patterns:

1. **Schema migration** follows the nxs-004 v0->v1 pattern (scan-and-rewrite, LegacyEntryRecord, schema_version counter). **PASS.**

2. **Table design** follows TAG_INDEX pattern for FEATURE_ENTRIES (multimap). **PASS.**

3. **Combined transaction pattern** from vnc-002/vnc-003 is adapted for the two-transaction retrieval approach (ADR-001). This is a deliberate deviation from the mutation pattern (where everything is in one txn) because usage recording is analytics, not critical data. **PASS -- deviation is documented and justified.**

4. **Server state** adds UsageDedup to UnimatrixServer. Since UnimatrixServer is Clone (required by rmcp), UsageDedup must be Arc-wrapped. The architecture addresses this (Arc<UsageDedup> with internal Mutex). **PASS.**

5. **Fire-and-forget** (ADR-004) means usage recording errors don't fail tool calls. Risk R-09 addresses the masking concern with end-to-end integration tests. **PASS.**

6. **Vote correction** (ADR-006) ensures decrement-old and increment-new happen in the same write transaction. The store gains two additional slice parameters (decrement_helpful_ids, decrement_unhelpful_ids) for this purpose. **PASS -- clean separation: UsageDedup detects corrections, store applies them atomically.**

7. **Trust gating** (ADR-007) enforces FEATURE_ENTRIES access control at the server layer's `record_usage_for_entries` method. Single enforcement point, consistent with existing capability checks. **PASS.**

8. **7 ADRs** cover all key decisions. ADR-001 through ADR-005 are unchanged from the initial design. ADR-006 and ADR-007 are new, addressing the security review findings. **PASS.**

### Specification Review

The specification covers all 18 acceptance criteria from SCOPE.md (AC-01 through AC-18). Mapping:

| SCOPE AC | Spec FR | Covered |
|----------|---------|---------|
| AC-01 | FR-03 | Yes |
| AC-02 | FR-01 | Yes |
| AC-03 | FR-02 | Yes |
| AC-04 | FR-04, FR-08 | Yes |
| AC-05 | FR-05 | Yes |
| AC-06 | FR-06, FR-09 | Yes |
| AC-07 | FR-07, FR-09 | Yes |
| AC-08 | FR-11, FR-21 | Yes (trust gating included) |
| AC-09 | FR-11 (idempotent) | Yes |
| AC-10 | FR-12 | Yes |
| AC-11 | FR-16 | Yes |
| AC-12 | FR-18 | Yes |
| AC-13 | FR-19, FR-04 | Yes |
| AC-14 | FR-14, FR-20 | Yes (includes decrement atomicity) |
| AC-15 | FR-08, FR-09, FR-10 | Yes (HashMap for votes documented) |
| AC-16 | FR-09, FR-20 | Yes (vote correction + atomicity) |
| AC-17 | FR-11, FR-21 | Yes (trust gating for FEATURE_ENTRIES) |
| AC-18 | Spec AC-18 | Yes (tests include vote correction and trust gating) |

No scope gaps. **PASS.**

### Risk Strategy Review

The risk strategy identifies 16 risks with 66 scenarios. Key coverage:

- **Migration risk (R-01)**: 8 scenarios including chain migration (v0->v1->v2). Covers the highest-severity data integrity risk. **PASS.**
- **Gaming resistance (R-03)**: 8 scenarios covering dedup correctness. Maps directly to SCOPE's gaming resistance research. **PASS.**
- **Vote correction atomicity (R-16)**: 5 scenarios covering vote flip, repeat same-value vote, reverse flip, saturating subtraction edge case, and batch correction. **PASS.**
- **Trust bypass (R-17)**: 4 scenarios covering Restricted, Internal, and Privileged agents plus result-unchanged verification. **PASS.**
- **Security risks (SR-01 through SR-05)**: Addresses read-path side effects, helpful flag abuse, feature parameter injection, information disclosure, and active suppression via unhelpful voting. SR-01 and SR-03 updated for trust gating. SR-05 is new (active suppression). **PASS.**
- **Fire-and-forget masking (R-09)**: End-to-end integration test required. **PASS.**

The risk strategy does not cover:
- Performance regression from adding write transactions to reads. This is addressed by NFR-01 in the specification (10ms per batch) but not as a named risk. **Minor gap -- covered by NFR, not a risk-level concern.**

**PASS.**

## WARN Details

### W1: EntryStore Trait Extension

**What**: The architecture adds `EntryStore::record_access` trait method. The product vision's crt-001 description does not mention trait changes -- it focuses on tables and fields.

**Why it matters**: Trait changes affect the core crate (unimatrix-core) interface that downstream features depend on. Adding methods to a trait is backward-compatible but increases the implementation surface for any future alternative store implementations.

**Recommendation**: Accept. The trait extension is explicitly requested in SCOPE.md Goal #7 and follows the established pattern (EntryStore already has 16 methods). One additional method is proportionate.
