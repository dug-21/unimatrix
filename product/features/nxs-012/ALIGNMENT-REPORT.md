# Alignment Report: nxs-012

> Reviewed: 2026-05-27
> Artifacts reviewed:
>   - product/features/nxs-012/architecture/ARCHITECTURE.md
>   - product/features/nxs-012/specification/SPECIFICATION.md
>   - product/features/nxs-012/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Upholds all vision non-negotiables: hash chain integrity, immutable audit log, ACID storage, graceful degradation |
| Milestone Fit | PASS | Nexus-phase storage infrastructure; closes migration gap that blocks W2-1 container deployment readiness |
| Scope Gaps | PASS | All SCOPE.md goals (1–10) and acceptance criteria (AC-01 through AC-31) addressed in source docs |
| Scope Additions | WARN | SPECIFICATION.md FR-17 (record_provenance for new table counts) is not in SCOPE.md acceptance criteria |
| Architecture Consistency | PASS | Architecture maps cleanly to specification FRs; all ADR decisions are consistent |
| Risk Completeness | WARN | SR-09 traceability label correct; R-14 (transaction isolation gap) has no corresponding scope risk entry — coverage gap in traceability table, not in test coverage |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `graph_edges.id` omitted from export | SCOPE.md Goal #1 ("Export all rows") is clarified by Constraint #5 (id omission rationale). Architecture ADR-005 documents the reasoning. Consistent across all three source docs. |
| Simplification | `cycle_events.goal_embedding` excluded | SCOPE.md Non-Goal #1 explicitly excludes it. Consistent across all three source docs. |
| Simplification | `observation_metrics` / `observation_phase_metrics` cleared on `--force` | SCOPE.md Non-Goal #4 excludes these from the export contract but SCOPE.md Proposed Approach (step 4) explicitly lists their deletion in `drop_all_data`. Authorization is present in SCOPE.md; not a scope addition. |
| Addition | FR-17: `record_provenance` includes counts for new tables | SCOPE.md has no acceptance criterion for provenance record content. SPECIFICATION.md adds FR-17 as a functional requirement and AC is listed as AC-28 in spec (matching count reporting), but no corresponding SCOPE.md AC. Minor justified addition — provenance completeness is a vision non-negotiable (immutable audit log). |

## Variances Requiring Approval

No VARIANCE or FAIL findings. Two WARNs are informational and do not require approval.

### WARN 1: FR-17 scope addition — record_provenance for new table counts

**What**: SPECIFICATION.md FR-17 requires the `record_provenance` audit log entry to include counts for `graph_edges`, `observations`, and `cycle_events` alongside existing table counts. SCOPE.md contains no acceptance criterion for this behavior.

**Why it matters**: This is a minor functional addition. It is directly aligned with the vision non-negotiable for immutable audit log completeness. The addition is low-risk and low-effort, but it was not explicitly requested.

**Recommendation**: Accept as-is. The addition is trivially justified by the vision's audit log completeness principle. No approval action required unless the human wants strict scope adherence enforced.

---

### WARN 2: R-14 traceability gap in risk strategy

**What**: The RISK-TEST-STRATEGY.md Scope Risk Traceability table covers SR-01 through SR-09 from SCOPE-RISK-ASSESSMENT.md. Risk R-14 (transaction isolation gap — new export queries outside the DEFERRED snapshot) has no corresponding SCOPE-RISK-ASSESSMENT.md entry. The traceability table omits R-14. The test scenarios for R-14 are present and correct; only the traceability mapping is missing.

**Why it matters**: The traceability table's purpose is to confirm every scope risk is addressed by at least one architecture risk. The reverse — that every architecture risk maps back to a scope risk — is not required. R-14 is a pure architecture risk discovered during design, not a scope risk. This is expected behavior for the traceability table format, not a gap in coverage.

**Recommendation**: Accept as-is. R-14 test coverage is present (two scenarios). The traceability table does not need to include non-scope-risk entries. No document change needed.

---

## Detailed Findings

### Vision Alignment

The product vision states these non-negotiables: hash chain integrity, immutable audit log, ACID storage, single binary, zero infrastructure, graceful degradation. nxs-012 upholds all of them.

**Hash chain integrity**: Import calls `validate_hashes` post-ingest (existing pipeline, unchanged). The `--skip-quarantined` flag does not bypass hash validation — AC-31 / FR-29 explicitly require that a filtered export produces a *valid* hash covering the filtered rows, so import with hash validation enabled succeeds without `--skip-hash-validation`. Architecture C5 documents this: "The export file produced with `--skip-quarantined` has valid hash integrity because the footer hash covers exactly the filtered rows."

**Immutable audit log**: RISK-TEST-STRATEGY.md R-21 explicitly protects `audit_log` from inadvertent skip-quarantined filtering. The architecture's cascade table excludes `audit_log`. ADR-008 states this as a design constraint. The vision's append-only audit log requirement is enforced at multiple layers.

**ACID storage**: Export uses `BEGIN DEFERRED` (read snapshot); import uses `BEGIN IMMEDIATE` (write lock on single connection). New tables participate in the existing transactions. No new transaction boundaries introduced.

**Graceful degradation**: SR-03 / R-15 document `context_briefing` behavior with NULL `goal_embedding` post-import. The specification confirms the existing graceful degradation path handles this correctly. No new code required.

The feature directly addresses the "No backup/recovery story" gap (High severity, listed in product vision Critical Gaps under Scalability & Architecture). A complete export/import cycle is the prerequisite recovery path for W2-1 container deployment. nxs-012 closes the semantic data loss gap that made workstation migrations lossy — graph edges, phase affinity observations, and cycle history were previously destroyed on each export/import cycle.

### Milestone Fit

nxs-012 is a Nexus-phase (`nxs`) storage infrastructure feature. It:

1. Closes the migration loss gap that blocks complete workstation rebuilds — a practical prerequisite for W2-1 container deployment readiness
2. Does not build any Wave 1A intelligence features (no GNN, no phase-conditioned scoring, no session context)
3. Does not build any Wave 2 features (no container, no HTTP transport, no OAuth)
4. Adds `--skip-quarantined` as operational export tooling appropriate at any wave

No future-wave capabilities are being built prematurely. The milestone assignment is correct.

### Architecture Review

The architecture follows the nan-001/nan-002 established patterns and is internally consistent.

**C1 (format.rs)**: Three new row structs and `ExportRow` variants. Field-level decisions (id omission for `graph_edges`, `goal_embedding` exclusion) are fully traced to ADRs-005 and -004 respectively.

**C2 (export.rs)**: Three new export functions with correct column mappings. NaN safety for `graph_edges.weight` via `Number::from_f64` with 1.0 fallback (ADR-003). Nullable `metadata` via `nullable_text` helper. The skip-quarantined filter correctly shows `export_observations` and `export_cycle_events` as unaffected (no entry ID references).

**C3 (inserters.rs)**: Plain INSERT for `graph_edges` to surface UNIQUE constraint violations (ADR-005). Explicit id preservation for `observations` and `cycle_events` matches the audit_log pattern (ADR-006).

**C4 (import/mod.rs)**: format_version validation: accept 1 and 2, reject 0 and 3+ (ADR-002). `drop_all_data` ordering: `observation_phase_metrics` before `observation_metrics` before `observations` — correctness-required for the `observation_phase_metrics` → `observation_metrics` FK, not for the `observation_metrics` → `observations` relationship (which has no FK). The ordering label "FK-safe" in ADR-001 is slightly over-broad but the ordering itself is correct.

**C5 (skip-quarantined)**: ADR-007 is correctly marked SUPERSEDED by ADR-008 in the architecture document. The HashSet is allocated only when `--skip-quarantined` is true — zero overhead on the default path. Dual-column checks for `co_access` (entry_a, entry_b) and `graph_edges` (source_id, target_id) are architecturally correct. The skip-set query runs inside the `BEGIN DEFERRED` transaction, before any `export_*` call, satisfying SR-02 (TOCTOU prevention).

The component interaction diagram accurately reflects both the export and import data flows, including the skip-quarantined branching and early abort on missing `--confirm`.

### Specification Review

All 31 acceptance criteria from SCOPE.md (AC-01 through AC-31) are present in SPECIFICATION.md. Functional requirements FR-01 through FR-29 map 1:1 to scope goals and acceptance criteria. The scope's 10 goals are each addressed by one or more FRs.

FR-17 (record_provenance includes new table counts) has no SCOPE.md acceptance criterion. This is flagged as WARN 1 above. The addition is justified by the vision's audit log completeness principle.

Non-functional requirements cover all relevant dimensions: export performance (NFR-01), import transaction duration (NFR-02), file size bounds (NFR-03), backward compatibility (NFR-04), snapshot isolation (NFR-05), skip-set memory overhead (NFR-06), and skip-filtering performance (NFR-07).

The hash validation interaction with `--skip-quarantined` is correctly specified: FR-29 / AC-31 require that filtered exports have *valid* hash integrity (hash covers filtered rows), so import succeeds without `--skip-hash-validation`. This is consistent with the architecture's C5 documentation and does not introduce a usability problem.

### Risk Strategy Review

The risk register contains 24 risks covering 65 test scenarios (per the Coverage Summary table). All 9 scope risks from SCOPE-RISK-ASSESSMENT.md are mapped in the Scope Risk Traceability table.

Notable strengths:

- R-16 through R-20 comprehensively cover the skip-quarantined cascade correctness risks, including separate tests for each of the two dual-column checks (R-19 for co_access, R-20 for graph_edges) and the full 5-exporter cascade integration test (R-16).
- R-21 explicitly protects audit_log from inadvertent filtering with its own test scenarios — directly aligned with the vision non-negotiable.
- R-23 correctly covers the `--confirm` bypass risk with all four flag combination scenarios (neither flag, skip-only, both, confirm-only).
- R-02 covers SR-06 (the highest-priority scope risk) with three distinct scenarios including the FK ordering edge case.

The SR-09 traceability entry is correct: SCOPE-RISK-ASSESSMENT.md SR-09 is about `--confirm` being a CLI flag (not interactive) for automation compatibility, and it maps to R-23 (confirm safeguard bypass). This is accurate.

R-14 (transaction isolation gap) has no corresponding scope risk entry. This is flagged as WARN 2 above. R-14's two test scenarios are present and cover the required verification. The traceability gap is documentation-only.

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns -- 5 results returned; none directly applicable to nxs-012. Patterns found relate to config key divergence, signal fusion ordering, affinity boost design, model registry, and search pipeline step ordering. nxs-012 is a storage/CLI feature with no ML or ranking components.
- Stored: nothing novel to store -- nxs-012 variances are feature-specific and do not represent recurring misalignment types. The FR-17 provenance addition is a one-off engineering judgment consistent with the audit log non-negotiable. The R-14 traceability gap is expected behavior (architecture risks need not trace back to scope risks). Neither pattern generalizes across features.
