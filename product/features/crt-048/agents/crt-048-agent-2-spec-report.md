# Agent Report: crt-048-agent-2-spec

**Agent:** crt-048-agent-2-spec (Specification Writer)
**Feature:** crt-048 — Drop Freshness from Lambda
**Output:** product/features/crt-048/specification/SPECIFICATION.md

---

## Deliverable

SPECIFICATION.md written to `product/features/crt-048/specification/SPECIFICATION.md`.

Covers:
- 18 functional requirements (FR-01 through FR-18)
- 6 non-functional requirements (NFR-01 through NFR-06)
- 14 acceptance criteria (AC-01 through AC-14) — expanded from SCOPE.md's AC-01 through AC-12
- Domain model: updated CoherenceWeights, Lambda definition, StatusReport removed fields,
  DEFAULT_STALENESS_THRESHOLD_SECS retention rationale, ubiquitous language table
- Two user workflows (context_status call, run_maintenance tick)
- 7 constraints (C-01 through C-07)
- Dependency table (4 affected files, all in unimatrix-server)
- 11 explicit NOT-in-scope exclusions
- 3 open questions for architect (OQ-A through OQ-C)

---

## Key Decisions Made

**AC-13 added (coherence_by_source)** — SR-05 from the risk assessment flagged that the
per-source Lambda path was not explicitly covered by the SCOPE.md ACs. AC-13 adds an
explicit acceptance criterion for the per-source call site update.

**AC-14 added (mcp/response/mod.rs compile)** — SR-06 flagged that the ~12 field removal
sites in mod.rs cause compile errors (not test failures). Added AC-14 to make this a
named acceptance criterion so the architect knows to enumerate all sites before pseudocode.

**FR-10 elevated (DEFAULT_STALENESS_THRESHOLD_SECS)** — SR-03 identified this as the
highest-severity scope risk: SCOPE.md Goal 7 says "remove if no other caller" but
Implementation Notes says retain it for run_maintenance(). The spec rewrites this as a
mandatory retention rule (FR-10) with the exact comment text specified. Goal 7 ambiguity
is resolved.

**NFR-04 (epsilon comparison)** — SR-01 risk about f64 sum representation is addressed
as a named NFR specifying the exact comparison form `(sum - 1.0_f64).abs() < f64::EPSILON`
and forbidding `==`.

**OQ-A (mod.rs exact line enumeration)** — Flagged as an open question for the architect
because the SCOPE.md estimate lists seven candidate line numbers for six sites. Architect
must enumerate before pseudocode.

---

## Interpretation Notes

- Weight literals (0.46, 0.31, 0.23) are locked per OQ-1 resolution in SCOPE.md.
  Spec treats them as fixed constants, not subject to re-derivation.
- The "clean removal, no migration window" decision (OQ-2) is carried into NFR-06 and
  C-07, requiring only release note documentation.
- `coherence_by_source` retention (OQ-3) is carried as FR-12 with the clarification
  that only the freshness argument is removed from the loop's `compute_lambda()` call.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADR-003 (entry #179, superseded
  by AC-12), pattern #4189 (structural dimension rationale), lesson #3704 (freshness
  half-life history). Entry #179 is the primary supersession target. Entry #4189 confirms
  the pattern-level rationale for the design. No new generalizable patterns to store
  from specification work — the pattern (#4189) was already written.
