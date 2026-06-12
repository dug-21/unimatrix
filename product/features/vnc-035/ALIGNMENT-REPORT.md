# Alignment Report: vnc-035

> Reviewed: 2026-06-12
> Artifacts reviewed:
>   - product/features/vnc-035/architecture/ARCHITECTURE.md
>   - product/features/vnc-035/specification/SPECIFICATION.md
>   - product/features/vnc-035/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-035/SCOPE.md
> Scope risk source: product/features/vnc-035/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md
> Goal source: Unimatrix entry #4677 (goal: self-learning intelligence)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Restores typed-graph integrity (architectural principle 4); advances `goal:self-learning` relational retrieval |
| Milestone Fit | PASS | Vinculum (MCP/connectivity) bug fix; no future-milestone capability built |
| Scope Gaps | PASS | All 6 Goals and AC-01..AC-11 carried into the source documents |
| Scope Additions | PASS | No capability added beyond SCOPE; ADRs are implementation detail, not scope expansion |
| Architecture Consistency | PASS | RESOLVED 2026-06-12 — carried-edge `created_at` reconciled to `now`/not-preserved across ADR-004 / SPEC FR-11 / RISK R-11 (was WARN) |
| Risk Completeness | PASS | All SR-01..SR-08 traced to R-01..R-11; dominant SR-01/AC-07 mandated by name across all three docs |

Counts: PASS 6, WARN 0, VARIANCE 0, FAIL 0.

> **Closure note (2026-06-12):** The single WARN raised in the original review (carried-edge
> `created_at` cross-document contradiction) is **RESOLVED** — reconciled to `created_at = now`
> across ADR-004 / SPEC FR-11 / RISK R-11; no preservation, no provenance marker. ADR-004 now
> carries an explicit "Carried edge metadata" subsection as the single authority; RISK R-11 was
> reframed to the inverse risk (accidental preservation) with a test asserting
> `created_at == correction timestamp`. Overall stance unchanged: aligned, no blocking variances.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | All SCOPE Goals 1-6 and AC-01..AC-11 appear in SPEC FR-01..FR-12 / AC table and in ARCH components. |
| Addition | (none) | ADR-001..005, `CarrySummary`, `query_outgoing_edges`, step 8b′ are implementation mechanics within scope, not new product capability. |
| Simplification | Provenance: `source="agent"`, no DB marker (OQ-03) | Rationale documented: awareness delivered via `edges_carried` ack (AC-11), not schema. Accepted one-way door, consistent across SCOPE/SPEC. |
| Deferral | Corpus repair sweep (OQ-05) | Cleanly deferred to a separate follow-up GitHub issue in all docs. Zero test scenarios attached. Consistent. |
| Deferral | `source_id` index (O-1) | Latency-only open question; ARCH marks "developer to resolve during delivery, not a blocker"; RISK R-09 requires only presence-verification, no functional test. Consistent. |

## Variances Requiring Approval

None at VARIANCE or FAIL level. The single WARN raised in the original review is now **RESOLVED** (retained below for trace).

1. **[RESOLVED 2026-06-12]** **What** — Cross-document contradiction on whether the carried edge's `created_at` is preserved from the source row. **Reconciled to `created_at = now` / not-preserved across ADR-004 / SPEC FR-11 / RISK R-11; no preservation, no provenance marker.**
   - ARCHITECTURE ADR-004 now carries an explicit "Carried edge metadata (OQ-03 / FR-11 — settled)" subsection (lines 49-75): `created_at` = **`now`** (correction timestamp), Preserved? **No**; `created_by`/`source` = `"agent"`, re-stamped; "**No provenance marker.**" ADR-004 is the single cited authority and now states the policy directly.
   - SPECIFICATION FR-11 (lines 94-98): "no preservation of the original's `created_at`/`created_by`." Unchanged, consistent with OQ-03.
   - RISK-TEST-STRATEGY R-11 (lines 25, 138-145): **reframed to the inverse risk (accidental preservation)**. Its test (line 143) now asserts a carried edge's `created_at` equals the correction timestamp (`now`), **not** the original source row's — the exact opposite of the original "preserves" claim that produced the WARN.
2. **Why it mattered** — Principle-4 adjacent (typed-graph correctness); touched the OQ-03 "no provenance marker" resolution. The earlier opposite readings could not both pass a test. Now all three documents derive `created_at = now` from the same ADR-004 authority — the contradiction is gone.
3. **Resolution** — Aligned RISK to SPEC/SCOPE (the two authoritative statements) and pinned the policy explicitly in ADR-004 so downstream documents share one source. No further action required before delivery.

## Detailed Findings

### Vision Alignment — PASS

The feature directly serves architectural principle 4 ("Typed relationship graph … graph traversal surfaces what vector search alone cannot"). SCOPE documents a confirmed-live regression: goal entries (`personal-cloud`, `proactive-delivery`) lost their `Advances → vision_root` edge through correction, orphaning them from the vision graph. AC-03 closes exactly this regression. The fix preserves agent-declared relationships through correction, which is the relational substrate the `self-learning` goal (#4677) depends on ("behavioral signals … phase-conditioned category affinity … graph-relational retrieval"). The GH issue is correctly labeled `goal:self-learning`.

The "persist by default, explicit shed" inversion (SCOPE §Decision) aligns with the vision's curation-as-first-class principle: correction is a normal curation act and must not silently destroy declared relationships. No vision principle is violated. Hash-chain/audit principles (1, 2) are untouched — correction already chains; carry-forward only writes edges. Graceful-degradation (5) is honored via warn-and-continue (NFR-01/AC-07).

### Milestone Fit — PASS

vnc-035 is a Vinculum (MCP server / connectivity) bug fix against an existing shipped behavior (vnc-015/016/017). It builds no future-milestone capability. The "no ceiling" decision (AC-09) and the explicit refusal of a tuning knob (Non-Goal) actively resist gold-plating. The deferred corpus sweep (OQ-05) and provenance marker (OQ-03) are pushed out rather than pulled in — correct milestone discipline. No over-build observed; this is consistent with the memory note on avoiding overstated defensive structure (the docs reframe "no ceiling" safety as an eligibility-filter invariant, not a new guard).

### Architecture Review — PASS (cross-document inconsistency RESOLVED 2026-06-12)

The architecture is coherent and proportionate: a single additive pipeline step (8b′) between existing steps 8b and 8c, mirroring vnc-017's incoming path. Sequencing rationale (ADR-001) is sound — carry after `params.edges` for honest `INSERT OR IGNORE` dedupe/count, before incoming-redirect for `Contradicts` disjointness (ADR-005, addresses SR-06). The single-SQL eligibility predicate (ADR-002) correctly centralizes the agent-declared-only filter and documents the *superset* difference from `query_incoming_edges` to pre-empt a false-symmetry "fix" (SR-03). Reuse of vnc-015/017 primitives respects the cumulative-infra rule.

The `created_at` contradiction noted in the original review is **RESOLVED (2026-06-12)**: ADR-004 now carries an explicit "Carried edge metadata (OQ-03 / FR-11 — settled)" subsection that pins `created_at = now` (correction timestamp, **not** preserved), `created_by`/`source = "agent"`, and "**No provenance marker.**" ADR-004 is now the single authority all three documents derive from, and RISK R-11 has been reframed to the inverse risk (accidental preservation) with a test asserting `created_at == correction timestamp`. ARCH, SPEC, and RISK agree.

Open questions O-1 (source index) and O-2 (module split) are appropriately deferred to delivery and do not affect correctness — acceptable.

### Specification Review — PASS

FR-01..FR-12 map 1:1 to SCOPE Goals and AC-01..AC-11; IDs are preserved end-to-end. The AC table gives every AC an explicit verification method. AC-07 is correctly elevated to MANDATORY with a named test and a Gate-3b-by-name requirement, directly honoring SR-01 and lesson #4473 (vnc-017's identical AC silently omitted → Gate 3b FAIL). AC-10+AC-11 are coupled as one acceptance unit (SR-05); AC-04+AC-09 share the eligibility predicate (SR-03/SR-04). "NOT in Scope" enumerates every SCOPE Non-Goal plus the accepted one-way doors. The spec's only contribution to the WARN is FR-11's `created_at` clause, which is internally consistent with SCOPE OQ-03 — the divergence lives in RISK, not here.

### Risk Strategy Review — PASS (R-11 reconciled 2026-06-12)

The risk strategy is thorough: 11 risks, 27 scenarios, every SR-XX traced to at least one R-XX (Scope Risk Traceability table). The dominant risk R-01 carries the AC-07 mandatory named test forward with the #4473 precedent and a required fault-injection seam — the single most important gate-protection in this feature, correctly prioritized Critical. Security section correctly identifies R-03 as security-adjacent (predicate regression → unbounded fan-out, the risk vnc-017's ceiling guards on the incoming side). Edge-case and failure-mode tables are complete.

R-11, formerly the source of the WARN, is now reconciled (2026-06-12): it is reframed to the **inverse** risk — accidental *preservation* of the original's `created_at`/`created_by` — and its test (line 143) asserts a carried edge's `created_at` equals the correction timestamp (`now`), not the original. This agrees with SPEC FR-11 and ADR-004. The risk remains correctly rated Low/accepted. No risk is missing; no risk over-states defensive structure.
