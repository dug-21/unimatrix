# Scope Risk Assessment: nxs-012 (Revised — Export-Side Quarantine Filtering)

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `graph_edges.weight` (f64) NaN/Infinity values corrupt JSONL — `Number::from_f64` returns `None` for NaN | Med | Med | Reuse existing `Number::from_f64` fallback per pattern #1103; no new helper |
| SR-02 | Export-side quarantine filtering requires querying `entries` for status=3 IDs **before** the per-table export pass — this query must be inside the same `BEGIN DEFERRED` snapshot transaction to avoid TOCTOU races with concurrent quarantine operations | High | Med | Architect must ensure the skip-set query and all table exports share one snapshot transaction |
| SR-03 | `cycle_events.goal_embedding` BLOB exclusion means post-import goal-cluster affinity is unavailable until first cycle completion — silent quality degradation in `context_briefing` | Med | High | Document degradation window; confirm NULL goal_embedding graceful fallback |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | format_version 2 exports are unreadable by older binaries — one-way compatibility; users who downgrade lose import capability | Med | Low | Document in CLI help and export header; no code mitigation needed |
| SR-05 | `--skip-quarantined` produces a non-exact export — if user later expects to diff against a full export, hash mismatch is confusing. The `--confirm` safeguard mitigates accidental use but not post-hoc confusion | Med | Med | Export summary should report skip-quarantined was active and count of excluded entries; consider writing a metadata line in the export header |
| SR-06 | ADR-007 (#4614) describes an **import-side** HashSet design that is now stale — the scope moved filtering to export. Architect/spec must supersede or correct this ADR to avoid implementers following the wrong design | High | High | Correct or supersede ADR-007 before architecture begins; the HashSet concept transfers but the integration point changes from `ingest_rows` to `do_export` |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `drop_all_data` must clear 3 new tables; `observation_metrics`/`observation_phase_metrics` have FK cascades on `observations` — DELETE ordering must handle derived tables even though they are not exported | High | Med | Architect must map full FK cascade graph; delete derived metric tables before observations |
| SR-08 | Export-side filtering touches 5 table exporters (entries, entry_tags, feature_entries, co_access, graph_edges) — each must consistently check the same `HashSet<i64>` skip-set. A missed check in one exporter silently produces orphaned rows in the export file | High | Med | Spec should require an explicit skip-set check annotation per table exporter; round-trip test must verify no orphaned references to skipped IDs |
| SR-09 | `--confirm` safeguard interaction with scripted/automated exports — if confirmation is interactive-only (stdin prompt), CI pipelines and backup scripts cannot use `--skip-quarantined` | Med | Med | Require `--confirm` as a CLI flag (not interactive prompt) for automation compatibility; match nan-002 ADR-003 precedent (stderr warning, no interactive prompt) |

## Assumptions

1. **Retention GC bounds observation volume** (Non-Goals) — "5K-50K rows (< 5MB)" is assumed but not enforced at export time. If GC is disabled, export files may be unexpectedly large.
2. **goal_embedding reconstruction is lazy** (Resolved Questions) — Assumes all code paths handle NULL goal_embedding gracefully.
3. **Schema is v27 and stable** (Constraints) — No concurrent migrations modify target tables before delivery.
4. **Entry IDs are stable through import** (nan-002 pattern) — graph_edges depend on entry IDs matching post-import. Implicit dependency on nan-002 import behavior.
5. **Export ordering places entries before dependents** — The skip-set for `--skip-quarantined` is built by querying entries before exporting dependent tables. If export ordering changes, the skip-set may be incomplete.

## Design Recommendations

1. **SR-06 is the highest-priority risk** — ADR-007 is stale. The architect must correct it before designing the export-side filtering. The HashSet concept is sound but the integration point (export vs. import) and the query mechanism (pre-query vs. inline detection) differ fundamentally.
2. **SR-02 and SR-08 together define the core correctness constraint** — The skip-set must be built inside the snapshot transaction, and every entry-referencing table exporter must check it. Spec should make both constraints explicit acceptance criteria.
3. **SR-07 carries forward from the previous assessment** — FK cascade ordering in `drop_all_data` remains the most likely silent failure mode for the 3-new-tables work. Derived metric tables must be cleared even though they are not exported.
