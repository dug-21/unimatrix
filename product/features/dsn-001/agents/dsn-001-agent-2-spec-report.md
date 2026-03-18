# Agent Report: dsn-001-agent-2-spec (Revised)

## Output

- `/workspaces/unimatrix/product/features/dsn-001/specification/SPECIFICATION.md` — overwritten with full revised specification incorporating the `[profile]` preset system, exact weight values, exhaustive SR-05 rename checklist, and all 27 ACs from SCOPE.md.

## Key Decisions

### D-01: Preset weight table hardened with exact values from ADR-005

SCOPE.md left weight values as "architect deliverable." ADR-005 delivered exact values verified by sum checks. These are committed verbatim into AC-23 as a four-row table, eliminating SR-09. Delivery implements against exact numbers, not ordering relationships.

### D-02: Weight sum invariant is `(sum - 0.92).abs() < 1e-9`, not `<= 1.0`

SCOPE.md config schema comment says `sum must be <= 1.0`. ADR-004 and ADR-005 both explicitly override this. The specification calls out the discrepancy and makes the 0.92 invariant authoritative throughout (FR-04, AC-24, Domain Models).

### D-03: SR-05 rename checklist distinguishes live files from historical artifacts

The checklist identifies every live file requiring update by path and approximate line number. Historical completed-feature artifacts (col-002, vnc-005, etc.) and research documents are explicitly excluded as historical records — updating them would rewrite history, not fix a bug. The verification gate is `grep -r "context_retrospective" .` returning zero matches outside the excluded historical set.

### D-04: `[knowledge] freshness_half_life_hours` precedence enumerated as a four-row truth table

SR-11 flagged three sources of truth for one value. AC-25 renders the complete precedence as a four-row truth table (named/absent, named/present, custom/absent, custom/present). The custom+absent cell maps to a named `ConfigError` variant (`CustomPresetMissingHalfLife`). The single resolution site is `resolve_confidence_params()` (FR-10).

### D-05: `CycleConfig` removed from `UnimatrixConfig`

ADR-004 confirms the `CycleConfig` stub is removed in dsn-001, not retained. The prior specification version included it as a forward-compat stub; this is superseded. The specification reflects removal in FR-04, constraints, and the NOT in scope section.

### D-06: `confidence_params_from_preset` is a free function in `unimatrix-server`, not a method on `ConfidenceParams`

ADR-006 resolves the crate dependency problem: `ConfidenceParams` lives in `unimatrix-engine` and must not depend on `Preset` (a server type). The conversion function and the SR-10 test both live in `unimatrix-server/src/infra/config.rs`. Captured in FR-10, AC-22, AC-21.

### D-07: Historical artifact exclusion policy is explicit in the specification

The full grep sweep returns ~235 files. Most are completed-feature and research documents. The specification makes the exclusion policy explicit so delivery neither skips live files nor wastes time renaming historical records.

## Open Questions

None. All risks from SCOPE-RISK-ASSESSMENT.md are resolved by the architecture:
- SR-02: `ConfidenceParams` extended to 9 fields (ADR-001) — FR-04.
- SR-09: Exact preset values committed in ADR-005 and AC-23 table.
- SR-10: Mandatory SR-10 test codified in AC-21.
- SR-11: Four-row precedence table in AC-25; single resolution site in FR-10.
- SR-12: `ConfidenceConfig` promoted from stub to live section (ADR-004) — FR-03.
- SR-13: W3-1 unblocked by AC-27 asserting all nine `ConfidenceParams` fields are populated.

## Knowledge Stewardship

- Queried: /uni-query-patterns for config externalization specification AC patterns — no results (first externalization feature in this codebase; no prior patterns stored).
