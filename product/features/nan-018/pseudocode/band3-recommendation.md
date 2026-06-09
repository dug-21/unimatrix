# Component: Band-3 recommendation doc + Unimatrix convention/procedure — `band3-recommendation.md`

**Wave**: 2 (deferrable; ZERO code coupling to Wave-1 — NFR-04)
**Location**: `product/features/nan-018/RECOMMENDATION-band3-protocol.md` (new doc) +
Unimatrix `convention` + `procedure` entries. **ADR**: ADR-005 (#4893).
**Risks**: R-16 (boundary breach — HARD GATE), R-14.

## Purpose

Hand off a RECOMMENDATION for a conditional eval-corpus-migration protocol trigger — patterned on
the `[CONDITIONAL] uni-docs` step, firing when "your change alters the retrieval-shape hash"
(coupled to the Goal-6 hash). The recommendation is for a SEPARATE uni-zero ratification. nan-018
makes **NO `.claude/protocols/` edits** and wires NO eval-as-gate (AC-12a/AC-13 hard gate).

## Recommendation doc (`RECOMMENDATION-band3-protocol.md`, FR-27)

Describes how the design and delivery/bugfix protocols WOULD carry a conditional step:
- **Trigger predicate**: "your change alters the retrieval-shape hash" — deterministic, coupled to
  the ADR-002 hash (NOT an enumerated list). The same hash definition that powers the drift guard
  is the trigger predicate (OQ-4/OQ-5 unified — one definition).
- **Action when fired**: re-stamp the fixture corpus + update assertions per the migration runbook
  (`docs.md` Band-2 #2). Asset-MAINTENANCE only — explicitly NOT execution-gating.
- **Boundary statement**: this is a recommendation for separate uni-zero ratification; nan-018
  edits no protocol file and adds no standing eval gate. The one-time migration-validation run is
  allowed; a standing decision gate is not (Non-Goal #1).

## Unimatrix knowledge entries (FR-28/29 — ship inside nan-018)

- **`convention` (FR-28)**: couples "schema/shape change => corpus migration" — surfacable in
  briefing. States: when a change alters the retrieval-shape hash, the fixture corpus must be
  re-stamped and its assertions revalidated.
- **`procedure` (FR-29)**: (a) how to migrate the corpus (re-stamp, bump migration_number, bump
  MANIFEST_VERSION on input-set change, update assertions); (b) how to author a fixture scenario
  (shape choice, alias discipline, property-assertion authoring with the asymmetric rank-below rule).

## HARD GATE — boundary (AC-12a/AC-13, R-16 — LOAD-BEARING)

- **NO edits to ANY `.claude/protocols/` file.** `git diff` MUST show zero changes under
  `.claude/protocols/`.
- **NO eval-as-standing-gate wiring** — no CI/PR hook makes eval RESULTS a standing decision gate.
- The recommended trigger is asset-maintenance only, NOT execution-gating.
- The recommendation doc lives at `product/features/nan-018/RECOMMENDATION-band3-protocol.md`,
  handed to a later uni-zero session.

## Wave independence (NFR-04)

These artifacts (recommendation doc, convention, procedure) have ZERO code coupling to Wave-1.
Wave-1 acceptance (AC-01…09 + AC-14) passes with NONE of them present (R-14).

## Data flow / Error handling

Knowledge + doc artifacts; no runtime logic, no error paths.

## Key test scenarios

- **git-diff gate (R-16.1, AC-12a/13 — HARD)**: assert ZERO changes under `.claude/protocols/`.
- **No-gate-wiring (R-16.2)**: review confirms no CI/PR hook makes eval results a standing gate;
  the one-time migration-validation run is allowed.
- **Recommendation doc presence (R-16.3)**: doc exists at the stated path, recommendation-only.
- **Convention surfacable (AC-12b)**: the `convention` entry exists and surfaces in briefing.
- **Procedure entries (AC-12c)**: migration + scenario-authoring procedures exist.
- **Wave-1-alone (R-14, NFR-04)**: Wave-1 acceptance passes with these artifacts absent.
