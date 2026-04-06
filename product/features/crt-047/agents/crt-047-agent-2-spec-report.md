# Agent Report: crt-047-agent-2-spec

## Task
Write SPECIFICATION.md for crt-047 (curation health metrics).

## Output
`product/features/crt-047/specification/SPECIFICATION.md`

## Acceptance Criteria Count
18 ACs (AC-01 through AC-18).

## Key Decisions Made

### SR-01 Resolution (AUDIT_LOG operation string)
Verified in codebase before writing spec. `context_correct` writes `"context_correct"`
to AUDIT_LOG (`services/store_correct.rs:91`), not `"context_deprecate"`. The orphan
query's `superseded_by IS NULL` filter already excludes all chain-deprecations
independently, so `operation = 'context_deprecate'` is the correct and complete filter.
No secondary join needed.

### Two-mechanism attribution documented
Corrections use `feature_cycle` on the correcting entry (correct mechanism — new entry
is created during the active cycle). Orphan deprecations use AUDIT_LOG timestamp
within the cycle window (correct mechanism — entry's `feature_cycle` records creation,
not deprecation time). These are explicitly different and the spec documents why.

### force=true: three cases distinguished (SR-05)
Case A (current cycle snapshot): recomputed from ENTRIES + AUDIT_LOG.
Case B (historical cycle snapshot): same — enables schema_version=1 → 2 upgrade.
Case C (rolling aggregate): always fresh — never separately cached.

### Baseline ordering key: feature_cycle not computed_at (SR-07)
`ORDER BY feature_cycle DESC LIMIT N` ensures stable baseline windows across
force-recomputes (computed_at is mutable on overwrite).

### Two independent cold-start thresholds documented
σ available at 3 cycles (MIN_HISTORY). Trend available at 6 cycles (3+3 split).
These are independent — σ does not require trend to be available.

### unattributed orphans (SR-08): explicit exclusion documented
Deprecations outside cycle windows are silently excluded from all cycle counts.
`context_status` does not surface a separate unattributed count in this feature.
Documented as a known limitation, not a bug.

### SUMMARY_SCHEMA_VERSION blast radius (SR-04): documented
All historical cycle_review_index rows will show the advisory until force=true.
Spec recommends operators run force=true on historical cycles that matter after v24 deploy.

### Two open questions flagged
OQ-SPEC-01: whether to filter AUDIT_LOG on `outcome = Success` (recommended yes).
OQ-SPEC-02: whether `corrections_system` surfaces in JSON output (ADR-gated by architect).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned 17 entries. Top matches:
  crt-033 ADR-002 (SUMMARY_SCHEMA_VERSION policy), cycle_review_index column addition
  pattern (entry #4178), crt-033 ADR-001 (write_pool_server), vnc-003 ADR-003
  (deprecation idempotency). All confirmed constraint and pattern selections.
- Codebase verification performed for SR-01: read `services/store_correct.rs`,
  `mcp/tools.rs`, `server.rs`, and `infra/audit.rs` to confirm all deprecation
  write paths and their AUDIT_LOG operation strings.
