# Agent Report: col-023-agent-2-spec

## Output

- `/workspaces/unimatrix/product/features/col-023/specification/SPECIFICATION.md`

## Key Decisions Made

**SR-02 resolved as C-03**: `UniversalMetrics` typed struct is the canonical
representation. Option A (dual HashMap representation) is explicitly rejected as a
constraint. `MetricVector.universal` stays `UniversalMetrics`. This closes the
ambiguity the risk assessment flagged.

**SR-01 resolved as FR-04.5 + C-05**: The DSL supports exactly two operator types
(threshold count and temporal window). The scope's claim that `json_pointer` alone is
sufficient is corrected — temporal window rules require a `RuleDslEvaluator` host
struct, but no new crate dependencies. The constraint enumerates this explicitly.

**SR-07 resolved as FR-04.3 + C-07**: `source_domain` guard is a mandatory
implementation contract stated at spec level, not left as an implementation detail. This
appeared in both a functional requirement and a constraint.

**SR-08 resolved as AC-05**: "Fully functional for W3-1" is defined precisely:
W3-1 requires multi-domain event ingestion and correct `source_domain` gating in
detection rules. W3-1 does NOT require pre-built multi-domain production rules.
AC-05 synthetic coverage is sufficient for the W3-1 dependency gate.

**FR-04.1**: The `DetectionRule` trait signature does not change. This was an
important clarification — downstream implementors don't need to update trait bounds,
only the `ObservationRecord` type definition changes.

**FR-05.1 + C-03**: Explicitly chose Option B (retain typed struct, add extension
column) over Option A (dual representation) based on entry #2844 and the single-source-of-truth principle. Two live representations of the same 21 fields invite divergence.

## Scope Risks Addressed

| Risk | Treatment |
|------|-----------|
| SR-01 | DSL operators enumerated in FR-04.5; expressiveness gap closed by RuleDslEvaluator requirement in C-05 |
| SR-02 | Option A rejected in C-03; typed struct is canonical |
| SR-03 | AC-05 defined as synthetic coverage; W3-1 gate defined precisely |
| SR-04 | NFR-09 requires wave-based refactoring with compilation gates; OQ-02 flags this for architect |
| SR-05 | OQ-01 defers tool name to architect; SR-05 is not resolvable at spec time |
| SR-06 | OQ-03 defers verification of BaselineSet key format to architect |
| SR-07 | FR-04.3 and C-07 make source_domain guard a mandatory spec-level contract |
| SR-08 | AC-05 defines the W3-1 gate boundary explicitly |

## Open Questions Raised

- **OQ-01**: Which existing tool is extended for Admin runtime domain pack registration?
  Must be named before implementation.
- **OQ-02**: Full `HookType` callsite inventory and wave partition plan — architect work.
- **OQ-03**: Verify `BaselineSet.universal` serialized key format in stored rows before
  committing to no migration on OUTCOME_INDEX.

## Conflicts or Gaps Found

**Gap 1 — Admin override tool unspecified**: SCOPE.md says "extend an existing tool"
without naming it. This is a spec-architecture gap. Deferred to architect as OQ-01.
The spec does not block on this: FR-06 specifies the behavior contract; the architect
picks the vehicle.

**Gap 2 — "claude-code" pack categories**: The scope's TOML example shows 8 categories
matching `INITIAL_CATEGORIES` exactly. The spec makes this explicit in C-10 to prevent
any regression in knowledge storage for existing operators.

**Gap 3 — DSL expressiveness for temporal rules**: The scope claimed `json_pointer`
sufficiency (false for temporal window rules). FR-04.5 corrects this with the
`RuleDslEvaluator` struct requirement while staying within the "no new crate
dependencies" constraint.

## Knowledge Stewardship

- Queried: /uni-query-patterns for observation pipeline domain generalization — entries
  #2843, #2844, #2902, #377 found; all materially used in NFR and constraint sections.
- Queried: /uni-query-patterns for detection rule source domain guard — entry #261 found;
  confirmed cross-domain guard as a known security pattern.
- Queried: /uni-query-patterns for wave-based refactoring compilation gate — entries #377,
  #363, #340 found; NFR-09 cites entry #377 directly.
- Queried: /uni-query-patterns for UniversalMetrics representation — entries #632, #2844
  found; C-03 resolution based on ADR-001 and migration complexity entry.
