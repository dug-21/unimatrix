# Agent Report — vnc-047-agent-2-spec

**Artifact:** `product/features/vnc-047/specification/SPECIFICATION.md`

## Summary
Authored SPECIFICATION.md from SCOPE.md (all OQs resolved) + SCOPE-RISK-ASSESSMENT.md, then
applied two human-directed changes (2026-07-09). 12 functional requirements, 9 non-functional,
AC-01…AC-09 (+AC-02a, AC-03a-d, AC-05a-d), domain model with ubiquitous language, workflows,
constraints, dependencies, NOT-in-scope, open questions.

## Human-directed revisions (2026-07-09)
1. **Whole-set-once** replaces accumulate/per-row semantics. FR-6, AC-02, AC-02a redefined: first
   tag-bearing start locks the ENTIRE set; every later start (same/subset/superset/different) is a
   whole-set no-op; a tagless start does NOT lock (first tags win). Enforced by an EXISTS-guard in
   the cycle_start txn (not namespace parsing — value-opacity preserved). AC-02a verify flipped:
   start {arm:A} then {arm:B} → stored exactly {arm:A}; added {A,B} then {C} → exactly {A,B}; added
   tagless-then-tagged → later tags lock.
2. **Ack echo** added as best-effort SHOULD (FR-12 / AC-09), explicitly NON-GATING: start-with-tags
   → "N tags accepted at cycle start: [...]" (accept-for-recording, not a durability guarantee);
   non-start-with-tags → "tags ignored — only recorded at cycle start"; listener trace for
   wrote-set vs frozen-skip (operator-visible; frozen-skip NOT caller-returnable without a new
   interface). Uses the existing ack string — no new interface, no read-back API.

Kept both accuracy notes: GC protection **by omission** (FR-8/AC-04 reframed — cycle_tags absent
from the gc delete enumeration; regression test asserts it) and **no per-tag audit event emitted**
(FR-7). Assembled-path proof obligations on AC-02/AC-05 and the two schema-version cascades intact.

## Key decisions / interpretations
- Split the two schema-version bumps into two discrete NFR cascades (NFR-1, NFR-2) with
  per-path acceptance line-items (AC-03a-d, AC-05a-d) per SR-01 — never one lumped task.
- Marked AC-02 and AC-05 **[assembled-path]** with an explicit `proven_by` obligation to cite
  an assembled MCP→hook→listener→cycle_review test, not a store-only structural test (SR-08).
- Elevated SR-05 (first-write-wins silent no-op on changed re-issued tags) to its own
  intended-behavior AC (AC-02a).
- Added NFR-6 (silent-failure containment on absent/evicted session, SR-03/SR-07) and NFR-8
  (no back-fill of historical reviews, SR-10) as first-class constraints.
- namespace-by-convention defined as reader-only; explicitly NOT derived/validated at the
  cycle-tag write path (contrast the entry `context_tag` namespace derivation, which is unused).

## Open questions (carry-forward, non-blocking)
- Architect: confirm the concrete absent-session persistence mechanism (SR-07 / #4136 parity).
- Impl start: re-verify v31 and SUMMARY v6 are still free at HEAD (SR-02).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned vnc-045 ADRs as the direct tag-model
  precedent (ADR-008 #5609 authorization posture: Capability::Write gate + agent_id audit-only;
  ADR-009 #5610 audit-event shape; ADR-004 #5608 replace-as-first-class — mutation not used here).
  col-025 / vnc-030 cited as cycle-attribute + additive-Option-param precedents. No new patterns
  to store — spec decisions are feature-specific (read-only tier).
