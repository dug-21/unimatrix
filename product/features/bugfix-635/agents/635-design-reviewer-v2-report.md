# Agent Report: 635-design-reviewer-v2

## Task
Design review of proposed architectural fix for GH Issue #635 (config load path observability + category authority violation).

## Assessment
APPROVED WITH NOTES

## Blocking Finding
Category filtering direction must be explicitly specified: domain pack categories are filtered against the already-seeded allowlist (not the reverse). `add_category()` should be removed from public API to prevent authority bypass.

## Non-Blocking Findings
- ConfigProvenance struct recommended (not ConfigSourceStatus)
- Add FallbackToDefault variant for error-path provenance
- Shared startup helper must be a plain function, not trait/builder
- Both root causes must ship together (provenance without authority = misleading logs)
- Worst-case blast radius is LOW (allowlist already seeded before domain pack loop)
- No hot-path risk, no new security surface

## Artifacts
- GH comment: https://github.com/dug-21/unimatrix/issues/635#issuecomment-4534483561

## Unimatrix Entries Consulted
- #4589 (lesson: config authority model)
- #86 (ADR-003: CategoryAllowlist runtime-extensible HashSet)
- #3775 (ADR-001 crt-031: lifecycle policy constructor)
- #2904 (ADR-002 col-023: Domain Pack Registry via TOML)
- #4585 (pattern: INITIAL_CATEGORIES lockstep)
- #4587 (ADR-002 nan-011: config.toml verified defaults)
- #3715 (lesson: category retirement lockstep — deprecated)
