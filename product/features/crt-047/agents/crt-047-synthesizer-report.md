# crt-047 Synthesizer Report

Agent ID: crt-047-synthesizer
Date: 2026-04-06

## Deliverables Produced

- `product/features/crt-047/IMPLEMENTATION-BRIEF.md`
- `product/features/crt-047/ACCEPTANCE-MAP.md`

## Contradiction Resolutions Applied

### FAIL-01 (orphan attribution — ADR-003 governs)
SPEC FR-05, FR-06, AC-04, NFR-03, and the AUDIT_LOG join approach are superseded.
ENTRIES-only query using `updated_at` as deprecation timestamp proxy. No AUDIT_LOG join.
OQ-SPEC-01 and WARN-02 closed as vacuous.

### FAIL-02 (baseline ordering key — ADR-001 governs)
SPEC FR-08 (5 columns), FR-10 (`ORDER BY feature_cycle DESC`), and AC-14 (5-column
assertion) are superseded. Seven new columns in v24. `ORDER BY first_computed_at DESC`
with `WHERE first_computed_at > 0`. Two-step upsert required in `store_cycle_review()`.

### WARN-01 (`corrections_system` stored — ADR-002 governs)
SPEC FR-08 omission and OQ-SPEC-02 "ADR-gated" language superseded. Column is stored in
DDL, migration, struct, and all I/O. Informational in output; excluded from σ baseline.

## Status
COMPLETE
