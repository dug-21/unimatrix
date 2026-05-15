# Gate 3a Report: vnc-015

> Gate: 3a (Component Design Review)
> Date: 2026-05-15
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 8 components map to architecture decomposition; interfaces and technology choices consistent |
| Specification coverage | PASS | All 17 FRs and 10 NFRs have corresponding pseudocode; no scope additions |
| Risk coverage (test plans) | PASS | All 15 risks (R-01 through R-15) have mapped test scenarios meeting coverage requirements |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage; no contradictions |
| Knowledge stewardship | PASS | Architect report (vnc-015-agent-1b) now contains required `## Knowledge Stewardship` section with Stored: entries |
| ADR-007: 10 variants in enum body | PASS | All 10 variants present in relation-type.md enum body |
| ADR-007: 10 variants in as_str() | PASS | All 10 variants present in as_str() match arms |
| ADR-007: 10 variants in from_str() | PASS | All 10 variants present in from_str() match arms |
| ADR-007: RelatedTo in PPR positive | PASS | RelatedTo in positive_out_degree_weight and personalized_pagerank in ppr-expand.md |
| ADR-007: RelatedTo in BFS positive | PASS | RelatedTo in positive BFS set in ppr-expand.md |
| ADR-007: Advances absent from PPR | PASS | Explicitly excluded with "DO NOT add Advances or Motivates here" |
| ADR-007: Motivates absent from PPR | PASS | Explicitly excluded |
| ADR-007: 10×4 checklist in test-plan | PASS | Full compliance matrix in test-plan/relation-type.md with grep commands |
| R-02: redirect uses pool.begin() RAII | PASS | edge-write.md shows `pool.begin().await?` → Transaction RAII; all SQL against `&mut *txn` |
| R-04: bidirectional Contradicts pre-return | PASS | Both (A→B) and (B→A) written before function returns in validate_and_write_edges |
| R-10: default_rules() signature updated | PASS | detection-rule.md shows `(history: Option<&[MetricVector]>, stale_edges: Vec<(u64, u64)>)` |
| context_edge validation pipeline order | PASS | capability → source fetch → source status → self-ref → new_target_id check → edge type → target validation |
| No ownership check on context_edge | PASS | OwnershipViolation does not exist; security gate is Write capability + source status |
| edge-write.md source_id=0 sentinel removed | PASS | Function body no longer has sentinel conditional; INVARIANT comment states post-insert only |
| new_target_id rejection ordering (R-13) | PASS | UnexpectedNewTargetId check placed at Step 5, before edge type (Step 6) and target validation (Step 7) |
| Residual parameter comment ambiguity | WARN | edge-write.md signature still has `// 0 if called pre-insert...` on source_id; contradicted by INVARIANT immediately below |
| OVERVIEW.md data flow diagram | WARN | Still shows two-call pattern with source_id=0 as Phase A; edge-input-params.md shows correct single post-insert call |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**: All 8 components in the pseudocode map exactly to the 9 architecture components (Component 8 in architecture = configuration, which was dropped; the numbering shift is documented). Each pseudocode file references the correct file path, crate, and module. Technology decisions are consistent:

- `write_graph_edge` via `nli_detection.rs:78` reused as specified
- `pool.begin().await?` RAII pattern for redirect (lesson #2269 cited in edge-write.md)
- `Status::Quarantined` and `Status::Deprecated` from `unimatrix_store::schema` (not integer literals)
- `edge_write.rs` as a `pub(crate)` module extraction per ADR-005
- All ADR decisions (ADR-001 through ADR-010) reflected correctly in pseudocode

The OVERVIEW.md data flow diagram correctly depicts the two-phase flow and the context_edge 7-step pipeline. Component Interactions match ARCHITECTURE.md.

---

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**: Every functional requirement has corresponding pseudocode coverage:

| FR | Covered By |
|----|-----------|
| FR-01: EdgeInput on context_store | edge-input-params.md — StoreParams struct + Phase A/B pipeline |
| FR-02: EdgeInput on context_correct | edge-input-params.md — context_correct pipeline |
| FR-03: 10 new RelationType variants | relation-type.md — all 10 variants with 3 sites each |
| FR-04: 4 mandatory change sites | relation-type.md — 10×4 compliance matrix |
| FR-05: Validation pipeline pre-insert | edge-input-params.md + edge-write.md |
| FR-06: Edge write placement (after duplicate guard) | edge-input-params.md — Phase A before insert; Phase B skips if duplicate |
| FR-07: write_graph_edge three-case contract | edge-write.md — three-case contract table explicitly included |
| FR-08: Bidirectional Contradicts | edge-write.md — Contradicts double-write in validate_and_write_edges |
| FR-09: query_contradicts_edges_for_entry fix | contradicts-fix.md — OR-clause fix with caller audit |
| FR-10: 10×4 compliance matrix | relation-type.md — table present |
| FR-11: RelatedTo in PPR/BFS | ppr-expand.md — Sites A, B, C documented |
| FR-12: stale_dependency_edges | stale-dependency.md — SQL query + GraphCohesionMetrics field |
| FR-13: DependencyOnDeprecated rule | detection-rule.md — full rule implementation + default_rules() change |
| FR-14: Edge-write helper extraction | edge-write.md — edge_write.rs as new pub(crate) module |
| FR-15: EDGE_SOURCE_AGENT constant | edge-write.md — `pub(crate) const EDGE_SOURCE_AGENT: &str = "agent"` |
| FR-16: context_edge MCP tool | context-edge-handler.md — full 7-step validation + mode dispatch |
| FR-17: Target validation on all surfaces | edge-write.md validate_target; context-edge-handler.md Step 7 |

Non-functional requirements NFR-01 through NFR-10 are addressed:
- NFR-01 (backward compat): `serde(default)` on `edges` field
- NFR-02 (no migration): no migration mentioned
- NFR-03 (idempotent): INSERT OR IGNORE confirmed
- NFR-04 (validation before write): Phase A pre-insert
- NFR-05 (sequential writes): no batch optimization
- NFR-06 (partial-write blast radius): ADR-003 accepted posture documented
- NFR-07 (tools.rs line count): extraction to edge_write.rs resolves
- NFR-08 (Capability::Write): no new capability
- NFR-09 (pure graph operation): no embedding/confidence calls in context_edge
- NFR-10 (Contradicts atomic): both directions before handler returns

No scope additions found.

---

### Check 3: Risk Coverage (Test Plans)

**Status**: PASS

**Evidence**: All 15 risks from RISK-TEST-STRATEGY.md have mapped test scenarios. Critical risks (R-01 through R-05) all meet the minimum scenario counts specified:

| Risk | Minimum Required | Planned Count | Met |
|------|-----------------|---------------|-----|
| R-01 (from_str silent drop) | 10 round-trip + 10 Pass 2b | 10 + 10 per-variant individually named | Yes |
| R-02 (redirect transaction) | RAII code review + 3 redirect tests | RAII gate + 3 integration tests | Yes |
| R-03 (bool semantics) | 2 idempotent re-assert tests | 2 (non-Contradicts + Contradicts) | Yes |
| R-04 (Contradicts partial write) | Both directions per surface | 4 (edges param, context_edge add, remove, redirect) | Yes |
| R-05 (redirect data loss) | 1 rollback-on-failure test | 3 (bad target, quarantined new target, atomic success) | Yes |

High risks (R-06 through R-10):
- R-06: 3 tests (quarantined source, deprecated source, active baseline)
- R-07: 4 tests (source direction, target direction compat, bidirectional, caller regression)
- R-08: 2 tests (first-error-abort, latency advisory) + first-error-abort explicitly required
- R-09: 2 tests (post-insert actual ID for context_store, pre-operation for context_edge)
- R-10: 3 tests (count=23, rule fires, caller audit compile gate)

Coverage requirements from RISK-TEST-STRATEGY.md are satisfied per iteration 1 analysis. No changes to test plans in this iteration that affect coverage.

---

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**: All shared types defined in OVERVIEW.md match per-component usage with no contradictions:

- `EdgeInput { edge_type: String, target_id: u64 }` — consistent in OVERVIEW.md, edge-input-params.md, edge-write.md, context-edge-handler.md
- `EdgeParams { mode, source_id, edge_type, target_id, new_target_id: Option<u64> }` — consistent in OVERVIEW.md, edge-input-params.md, context-edge-handler.md
- `EdgeValidationError { UnknownType, SelfReferential, TargetNotFound, TargetQuarantined }` — consistent in OVERVIEW.md, edge-write.md; matches ARCHITECTURE.md and IMPLEMENTATION-BRIEF.md
- `EdgeDeleteError { StoreError(StoreError) }` — consistent
- `EdgeRedirectError { TargetNotFound, TargetQuarantined, TransactionError(sqlx::Error) }` — consistent
- `GraphCohesionMetrics.stale_dependency_edges: u64` — consistent in OVERVIEW.md, stale-dependency.md
- `DependencyOnDeprecatedRule { stale_edge_pairs: Vec<(u64, u64)> }` — consistent in OVERVIEW.md, detection-rule.md
- `EDGE_SOURCE_AGENT: &str = "agent"` — consistent in OVERVIEW.md, edge-write.md
- `default_rules(history: Option<&[MetricVector]>, stale_edges: Vec<(u64, u64)>)` — consistent in detection-rule.md and OVERVIEW.md data flow

Data flow matches architecture: context_cycle_review pre-queries stale pairs → passes to default_rules → DependencyOnDeprecatedRule::new(stale_edge_pairs) → detect().

---

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

| Agent | Report File | Stewardship Section | Status |
|-------|------------|---------------------|--------|
| Architect (revision) | vnc-015-agent-1b-architect-revision-report.md | Present — `## Knowledge Stewardship` with Stored: (ADR-002 #4419→#4426, ADR-009 #4427, ADR-010 #4428) + Queried: | PASS |
| Risk Strategist | vnc-015-agent-3-risk-report.md | Present — Queried (4 queries) + Stored (nothing novel, with reason) | PASS |
| Pseudocode agent | vnc-015-agent-1-pseudocode-report.md | Present — Queried (3 queries) + Stored (nothing novel; all patterns already in Unimatrix) | PASS |
| Spec writer | vnc-015-agent-2-spec-report.md | Present — Queried (1 briefing query) | PASS |
| Test plan agent | vnc-015-agent-2-testplan-report.md | Present — Queried (2 queries) + Stored (nothing novel, with reason) | PASS |
| Synthesizer | vnc-015-synthesizer-report.md | MISSING `## Knowledge Stewardship` section | WARN |

The architect revision report (vnc-015-agent-1b-architect-revision-report.md) now contains a properly structured `## Knowledge Stewardship` section with:
- `Stored:` entries for all three ADR Unimatrix operations (ADR-002 supersession #4419→#4426, ADR-009 new #4427, ADR-010 new #4428)
- `Queried:` entry documenting context_search queries prior to revision

The synthesizer WARN from iteration 1 remains. The synthesizer is a coordinator role, not a design-phase specialist, and performs no direct Unimatrix storage operations. This does not block gate passage.

---

### ADR-007 Specific Checks: 10×4 Variant × Site Compliance

**Status**: PASS (unchanged from iteration 1 — no modifications to relation-type.md or ppr-expand.md)

All 10 variants confirmed in all 4 sites. Advances and Motivates confirmed absent from PPR positive sets. Full 10×4 checklist table present in test-plan/relation-type.md.

---

### R-02 Critical Check: redirect_graph_edge RAII Transaction

**Status**: PASS (unchanged from iteration 1)

`pool.begin().await?` → RAII Transaction; all 4 SQL statements against `&mut *txn`. No raw BEGIN/COMMIT strings. Rollback on drop for all error paths.

---

### R-04 Critical Check: Bidirectional Contradicts Before Return

**Status**: PASS (unchanged from iteration 1)

Both `(A, B, Contradicts)` and `(B, A, Contradicts)` written in the same function execution before it returns. context_edge add mode also independently writes both directions.

---

### R-10 Critical Check: default_rules() Signature

**Status**: PASS (unchanged from iteration 1)

`default_rules(history: Option<&[MetricVector]>, stale_edges: Vec<(u64, u64)>)` — updated signature in detection-rule.md. Count test updated to assert 23 rules.

---

### Fix Verification: edge-write.md source_id=0 Sentinel

**Status**: PASS with WARN

**Evidence**: The `validate_and_write_edges` function body no longer contains the `source_id=0` conditional branch or the two-call comment from iteration 1. The function body begins with:

```
// INVARIANT: Called post-insert only. Caller performs Phase A (type resolution + target
// validation) inline in the handler before entry insert. This function receives the
// actual post-insert source_id and performs the self-ref check + writes.
```

The self-ref check now uses `source_id` directly (the actual post-insert id) with no sentinel test.

**Residual WARN**: The function signature parameter comment still reads `// 0 if called pre-insert for type+target validation only`, which directly contradicts the INVARIANT comment immediately below it. A rust-dev implementing this function will see the contradictory comment first. The OVERVIEW.md data flow diagram (lines 22-34) also still shows `validate_and_write_edges(store, 0, edges, created_at)` as the Phase A call, which contradicts edge-input-params.md (which correctly shows the inline loop and a single post-insert call).

**Assessment**: The implementation body is unambiguous — no sentinel branch, no two-call pattern. The edge-input-params.md handler pipeline (the authoritative per-component source for the caller) correctly shows the single post-insert call pattern. A rust-dev implementing context_store will follow edge-input-params.md for handler logic and edge-write.md for the helper function body — both are now consistent at the implementation level. The OVERVIEW.md diagram and the signature comment are advisory artifacts. Delivery risk is low but not zero.

---

### Fix Verification: context_edge new_target_id Ordering (R-13)

**Status**: PASS

**Evidence**: context-edge-handler.md now places the UnexpectedNewTargetId check at Step 5, before edge type resolution (Step 6) and target validation (Step 7):

```
// ── Step 5: new_target_id presence check (R-13) ──────────────────────────
// Reject before edge type and target validation to give callers the most actionable error.
IF (params.mode == "add" OR params.mode == "remove") AND params.new_target_id.is_some() THEN
    RETURN error UnexpectedNewTargetId
    // "new_target_id is not valid for mode '{mode}'"
END IF

// ── Step 6: Edge type resolution ─────────────────────────────────────────
// ── Step 7: Target validation ─────────────────────────────────────────────
```

The state machine diagram at the bottom of context-edge-handler.md also reflects the corrected order: `[self-ref check] → [new_target_id presence check] → [edge type resolution] → [target validation]`.

The prior WARN (R-13 check placed after target validation, causing misleading error for add/remove with invalid target AND new_target_id present) is resolved. An add call with both an invalid target and a spurious new_target_id now returns `UnexpectedNewTargetId` rather than `TargetNotFound`.

---

### context_edge Validation Pipeline Order

**Status**: PASS

The corrected pipeline as documented in context-edge-handler.md:
1. Capability gate (Capability::Write)
2. Source fetch (store.get_entry_by_id)
3. Source status (Status::Quarantined OR Status::Deprecated → SourceFrozen)
4. Self-ref check (source_id == target_id)
5. new_target_id presence check (UnexpectedNewTargetId for add/remove)
6. Edge type resolution (RelationType::from_str)
7. Target validation (validate_target)

This matches ARCHITECTURE.md Component 9 pipeline and ADR-009.

---

### No Ownership Check

**Status**: PASS (unchanged from iteration 1)

context-edge-handler.md explicitly states `OwnershipViolation` does not exist. Consistent with ARCHITECTURE.md, IMPLEMENTATION-BRIEF.md Constraint #10, AC-22, and ADR-009.

---

### WARN: Residual Ambiguity in validate_and_write_edges

**Status**: WARN

Two residual artifacts create minor delivery ambiguity:

1. **Signature comment** (edge-write.md line 92): `// 0 if called pre-insert for type+target validation only` — directly contradicts the INVARIANT comment below it.
2. **OVERVIEW.md data flow** (lines 22-34): Shows `validate_and_write_edges(store, 0, edges, created_at)` as Phase A — contradicts the single post-insert call pattern in edge-input-params.md and the INVARIANT in edge-write.md.

**Assessment**: The function body is correct. The per-component handler spec (edge-input-params.md) is correct. A rust-dev following the component specs will implement correctly. The OVERVIEW.md and signature comment are navigation artifacts that create noise but do not override component-level specs. This is a low-priority cleanup for the rust-dev agent to resolve during implementation.

---

## Rework Required

None. All FAIL items from iteration 1 are resolved. Remaining WARNs are advisory and do not block delivery.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the pattern (partial fix leaving contradictory navigation artifacts) is a specific instance of the known "pseudocode fix incomplete across all files" lesson. This is feature-specific; the general lesson is already in Unimatrix.
- Queried: N/A (gate validator does not query before validating design artifacts)
