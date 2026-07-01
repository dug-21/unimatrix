# Alignment Report: vnc-042

> Reviewed: 2026-07-01
> Artifacts reviewed:
>   - product/features/vnc-042/architecture/ARCHITECTURE.md
>   - product/features/vnc-042/specification/SPECIFICATION.md
>   - product/features/vnc-042/RISK-TEST-STRATEGY.md
>   - product/features/vnc-042/architecture/ADR-001-follow-supersessions-param-default.md
>   - product/features/vnc-042/architecture/ADR-002-dead-end-fail-loud.md
>   - product/features/vnc-042/architecture/ADR-003-response-construction.md
> Scope source: product/features/vnc-042/SCOPE.md · product/features/vnc-042/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md · goals #4671/#5219/#4673/#4678/#4946 · capability #5230 (SLN3)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly serves the vision's "trustworthy, consistent" retrieval promise on the most-used read tool; honors principles #4 (typed graph) and #5 (fail-loud). |
| Goal / Capability Visibility | WARN | Aligns with self-learning goal #5219 but does NOT advance a tracked capability `done_when`. SLN3 (#5230) is write-side; vnc-042 is read-side. Lands capability-invisible. See routed decision below. |
| Milestone Fit | PASS | Surgical single-tool change, reuses existing primitives, defers NG-1/NG-2. No future-milestone capability pulled forward. |
| Scope Gaps | PASS | All six SCOPE goals + AC-01..AC-07 + all three open questions (OQ-1/2/3) addressed across arch/spec/ADRs. |
| Scope Additions | PASS | AC-08 (orphaned/quarantined footer well-formedness) is edge-case hardening of AC-03, explicitly declared not to alter the locked set. No unrequested capability. |
| Architecture Consistency | PASS | ADR-001/002/003 consistent with spec FR-01..FR-14 and risk R-01..R-12. The one prose tension (SCOPE FR-01 "bool default true" vs ADR-001 `Option<bool>`) is explicitly caught and ruled in R-02. |
| Risk Completeness | PASS | SR-01..SR-07 → R-01..R-12 with coverage plan; vision principle #5 covered by R-04; security section bounds DoS via hop cap. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | SCOPE Goals 1-6, NG-1..NG-5, AC-01..AC-07, OQ-1/2/3 all traced into arch/spec/ADR. |
| Addition | AC-08 (spec-derived, R-08) | Orphaned/quarantined `superseded_by IS NULL` footer must be well-formed or absent (no `#{}`/panic). Hardening of AC-03's escape hatch, ruled in ADR-003 §4. Spec states it "does not alter the locked set." Acceptable — within-scope edge-case, not a new capability. |
| Addition | `format_single_entry_with_note` (new fn); `follow_to_current` `pub(super)`→`pub(crate)` widen | Implementation mechanisms required to satisfy AC-02/03/04 and AC-05 reuse. Not scope expansion. |
| Simplification | ADR-002 dead-end returns *requested* id, not the stop-id | Rationale: keeps AC-05 "no new chain-walk" clean; `follow_to_current` discards the stop-id. Documented, sound. |
| Simplification | NG-1 neighbor-target resolution deferred | Rationale: resolved get returns terminal's edge *list* but leaves edge *targets* unresolved; asymmetry made legible by the resolution notice. Documented follow-up. |

## Variances Requiring Approval

No blocking VARIANCE or FAIL. One WARN routed to the vision session for a human ruling (advisory, non-blocking):

1. **What**: vnc-042 is read-side supersession resolution (`context_get` resolves a deprecated id to its active terminal at read time). The self-learning goal's tracked integrity capability, **SLN3 (#5230)**, has a purely **write-side** `done_when`: "correcting a hot node carries/redirects its referrers and accumulates no orphan edges." vnc-042 therefore ships **capability-invisible** — it advances no tracked capability `done_when`.
2. **Why it matters**: Vision-alignment asks whether a feature advances a strategic goal. vnc-042 clearly advances the *vision statement's* consistency/trustworthiness promise (a read that returns stale deprecated content when a corrected version exists is a retrieval-surface inconsistency), and it is a read-side mirror of the same "integrity-consistent under correction" property SLN3 guards on the write side. But because the goal map encodes that property only write-side, the read-side dimension is untracked — future capability accounting will not see vnc-042 as delivering anything, risking an orphaned deliverable and an incomplete picture of the consistency guarantee.
3. **Recommendation**: **Accept and proceed** — the feature is LOCKED (#843), correctness-motivated, and vision-aligned on its face; do not block. Separately (advisory, owner = uni-zero/goal steward, not this feature): add a **read-side consistency clause** to SLN3 or a sibling capability under #5219 — e.g. "retrieval surfaces resolve corrected entries to their active terminal; no read returns superseded content silently." This makes read-side consistency a tracked capability rather than capability-invisible, and closes the read-side-clause question the scope-review routed here.

## Detailed Findings

### Vision Alignment
- PRODUCT-VISION line 7: "Unimatrix makes that knowledge **trustworthy, consistent**." `context_get` today silently returns superseded content for a durable deprecated id (SCOPE Problem Statement; live hit on capability C0 #5191→#5304). Resolving to the active terminal by default directly delivers the consistency promise on the surface the vision names first: "agents retrieve knowledge on demand (search, lookup, **get**)" (line 9).
- Architectural Principle #4 (typed relationship graph, traversable): supersession is a typed edge; vnc-042 traverses it at read via the existing recursive-CTE primitive (`follow_to_current`/`query_current_terminal`, #4468). Aligned — surfaces what a raw by-id read cannot.
- Architectural Principle #5 (graceful degradation, "absent/failed = previous behavior, not broken behavior"): ADR-002 dead-end path returns the requested entry with a loud non-active flag — never empty, never silent. Risk R-04 additionally routes a `follow_to_current` internal store error into the same fail-loud flag. Directly honored.
- Architectural Principle #7 (in-memory hot path, "never read from the database at query time"): **N/A** — this principle governs analytics-derived *search* data (the scoring/relevance hot path). `context_get` is an exact by-id fetch, always DB-backed; the supersession CTE walk is the mandated mechanism (#4468), not the analytics hot path. No violation.
- Goal advancement: self-learning goal #5219 success criterion "the typed knowledge graph stays integrity-consistent under correction" is worded write-side; vnc-042 is its read-side complement (see WARN above). Other goals (proactive-delivery, domain-agnostic, personal-cloud) not implicated — correctly not claimed.

### Goal / Capability Visibility
- SLN3 (#5230) `done_when` is write-side only; `delivered_by` vnc-035/#743, open items #744/#745 are all correction-propagation. vnc-042 does not touch that path (NG-5: no schema/storage change; `supersedes`/`superseded_by` are read-only consumers here).
- Consequence: correct scoping (the feature genuinely is read-side), but the deliverable is untracked in the capability map. Flagged WARN, resolution routed as an advisory goal-steward action — not a change this feature should make.

### Milestone Fit
- Scope is a "surgical single-tool contract change confined to the MCP server crate" (ARCHITECTURE §System Overview). No schema, no new SQL, no other-tool changes (NG-3/NG-4/NG-5). Reuses `follow_to_current` (AC-05, C-1) rather than building new capability.
- Future work explicitly deferred, not pulled forward: NG-1 (neighbor-target resolution) and NG-2 (chain/evolution view → already lives in `context_graph` mode `chain`). Milestone discipline holds.
- Duplication of `follow_to_current` (`graph_read_neighbors.rs:36` vs `graph_read_supersession.rs:122`) is flagged for future cleanup and deliberately not consolidated here — correct scope restraint.

### Architecture Review
- ADR-001 (`follow_supersessions: Option<bool>`, default true; accept divergence from graph's `resolve_supersessions` default false via distinct verb) — internally consistent; resolves OQ-1 and SR-06.
- ADR-002 (dead-end returns originally-requested id + loud flag) — resolves OQ-2; consistent with spec FR-08 and AC-04.
- ADR-003 (note in handler-side `format_single_entry_with_note`; edges rebuilt on `effective_id`; json `resolution` object present only when non-clean) — resolves OQ-3, SR-03, SR-04; consistent with spec FR-09/FR-11/FR-12 and risk R-01/R-03/R-06/R-07.
- Cross-doc tension correctly managed: SCOPE FR-01 prose says "bool default true"; ADR-001 rules `Option<bool>` with handler-owned default. Risk R-02 (Critical) explicitly states "ADR-001 (the ruling) governs; FR-01's prose does not override the shape" and elevates the serde-default footgun to a behavioral test. No unresolved contradiction.

### Specification Review
- FR-01..FR-14 each carry AC/constraint traceability; AC-01..AC-08 each have a concrete verification method; §6 partitions the test surface into must-stay-green canaries (TS-01/02), classify-and-migrate clusters (TS-03), and new tests (TS-04..TS-09). Byte-identity invariant (NFR-05) protected by FR-09 injection-point rule. ADR-owned decisions (D1/D2) referenced, not relitigated. No scope drift beyond AC-08 hardening noted above.

### Risk Strategy Review
- SR-01..SR-07 fully traced to R-01..R-12 with a Scope-Risk Traceability table. Vision principle #5 explicitly cited in R-04. Security section bounds the untrusted `id` walk by the 50-hop cap (DoS bound) and keeps `follow_supersessions` a plain `Option<bool>` (no #3728 coercion class). Accepted risks (R-09 non-code durable-id consumers, R-10 naming divergence, R-12 NG-1 asymmetry) are documented-not-gated with the R-09 human-flag correctly surfaced. Complete for the feature's blast radius.

## Knowledge Stewardship
- Queried: /uni-query-patterns for vision alignment patterns — surfaced #3742 (optional future branch must match scope-deferral intent; WARN if arch/risk diverge) and #2298 (config semantic divergence). Neither triggers here: vnc-042's deferrals (NG-1/NG-2) are cleanly documented with rationale in both architecture and risk docs, and there is no config-vs-vision divergence. Also confirmed SLN3 (#5230) is write-side, grounding the capability-visibility WARN.
- Stored: nothing novel to store — the capability-invisibility observation is vnc-042-specific (a single read-side feature under a write-side-worded capability) and does not yet recur across 2+ features; the governing test/serde patterns (#5383, #3774/#3817, #4468, #4303) already exist. No generalizable cross-feature misalignment pattern newly visible.
