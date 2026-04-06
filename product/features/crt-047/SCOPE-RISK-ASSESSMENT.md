# Scope Risk Assessment: crt-047

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | AUDIT_LOG join for orphan attribution may silently under-count if the `operation` string for deprecations is not consistently `"deprecate"` across all write paths (`context_deprecate`, `context_correct` chain-deprecation) | High | Med | Architect must verify the exact operation strings written by every deprecation path before designing the attribution query. Inconsistency here produces silent correctness errors, not crashes. |
| SR-02 | Schema v23→v24 migration: a parallel in-flight feature could claim v24 before crt-047 merges, requiring a retroactive version renumber across all design artifacts (evidence: #4095, crt-043) | Med | Med | SM must grep `CURRENT_SCHEMA_VERSION` immediately before delivery begins. Architect must note this as a pre-delivery check. |
| SR-03 | Three migration paths must all be updated: `db.rs`, `migration.rs`, and the legacy static DDL array. Missing any one leaves old-schema databases on the wrong schema (evidence: #4153) | High | Med | Architect must include all three paths in the migration design. Spec must require an integration test that opens a real v23 database through `Store::open()`, not just the migration function in isolation (evidence: #378). |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `SUMMARY_SCHEMA_VERSION` bump to `2` will surface advisories on every historical memoized record. Operators calling `context_cycle_review force=false` on any past cycle will see the advisory. Scope describes this as "already designed behavior" but the blast radius (all historical cycles, not just new ones) may surprise operators. | Med | High | Spec should document the breadth of advisory exposure and recommend operators run a batch `force=true` pass after deploying v24 if needed. |
| SR-05 | `force=true` semantics are layered: the snapshot columns are "write-once at review time" but re-computed from ENTRIES for the current cycle; the rolling aggregate is always recomputed from stored snapshots. This dual meaning of `force=true` risks spec ambiguity — does it re-derive the raw counts or only the aggregate? (SCOPE.md OQ-06) | Med | Low | Spec must define `force=true` behavior explicitly for (a) current cycle raw snapshot, (b) historical cycle raw snapshots, and (c) the rolling aggregate — as three distinct cases. |
| SR-06 | `services/status.rs` 500-line cap: adding a Phase 7c curation health block plus helper functions may push the file over the limit, requiring an extraction mid-implementation | Low | Med | Architect should pre-plan extraction to `services/curation_health.rs` rather than reacting to the limit during delivery. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | The rolling baseline reads `cycle_review_index` rows ordered by `computed_at DESC`. Rows from the same cycle re-reviewed with `force=true` will have an updated `computed_at`, potentially displacing the canonical ordering relative to other cycles. If `feature_cycle` ordering and `computed_at` ordering diverge, the baseline window becomes non-deterministic. | High | Med | Architect must decide the canonical ordering key for baseline window selection. `computed_at` is mutable; `feature_cycle` is the primary key. The design should use `feature_cycle` ordering (or the cycle start timestamp from `cycle_events`) rather than `computed_at` to ensure stable baseline windows. |
| SR-08 | Deprecation entries attributed to a cycle via AUDIT_LOG timestamp window: `context_deprecate` called outside an active cycle (human-initiated, no cycle running) produces an audit entry with a timestamp that does not fall in any cycle window. These entries will be unattributed and silently excluded from all cycle counts. | Med | High | Spec must document this exclusion explicitly. `context_status` aggregate view should surface unattributed orphan count separately, or the architect must decide it is acceptable to silently drop them. |

## Assumptions

| Assumption | SCOPE.md Section | Risk if Wrong |
|------------|-----------------|---------------|
| `context_deprecate` always writes an AUDIT_LOG entry with a consistent operation string (`"deprecate"`) | Background Research § trust_source, Constraints § orphan attribution | Orphan counts silently wrong — no crash or error |
| `superseded_by IS NULL` is the reliable discriminator for orphan status at query time | Background Research § corrections/deprecations on ENTRIES | A deprecation entry that was created with a supersession plan but the supersession was aborted would be mis-classified; no mechanism currently detects this |
| Fewer than N cycles with non-NULL snapshot data is the only cold-start state to handle | Non-Goals § backfilling, AC-08 | A DB with NULL snapshots interleaved with populated ones (e.g., partial backfill) could produce a baseline window with gaps; the baseline function must handle NULL rows gracefully |

## Design Recommendations

- **SR-01, SR-08**: Architect should enumerate all deprecation write paths and confirm AUDIT_LOG `operation` values before finalizing the orphan attribution SQL. A lookup against `audit.rs` and `store_correct.rs` is required.
- **SR-02**: SM pre-delivery check: `grep CURRENT_SCHEMA_VERSION crates/unimatrix-store/src/migration.rs` before pseudocode phase.
- **SR-03**: Design must explicitly cover all three migration paths (db.rs + migration.rs + legacy DDL). Spec must require a `Store::open()` integration test against a synthetic v23 database.
- **SR-07**: Use `feature_cycle` (primary key, stable) rather than `computed_at` (mutable) as the ordering key for baseline window selection.
- **SR-04, SR-05**: Spec must dedicate a section to `force=true` semantics covering both the snapshot recomputation scope and the advisory blast radius.
