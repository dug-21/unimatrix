# Agent Report: nxs-012-agent-3c-risk

## Task
Replace RISK-TEST-STRATEGY.md with fresh assessment reflecting the design change: --skip-quarantined moved from IMPORT to EXPORT (ADR-008 supersedes ADR-007). Retain R-01 through R-15 (3-table export risks), replace R-16+ with export-side skip-quarantined risks. Eliminate old R-23 (hash validation interaction) since export-side filtering preserves hash integrity by construction.

## What Changed

### Retained (R-01 through R-15)
All 15 original 3-table export/import risks unchanged. These cover NaN safety, FK-cascade ordering, format_version validation, ID preservation, transaction isolation, and goal_embedding degradation.

### Replaced (R-16 through R-24)
Old import-side skip-quarantined risks replaced with 9 export-side risks:

| Old Risk | New Risk | Change |
|----------|----------|--------|
| R-16 (import cascade incompleteness) | R-16 (export cascade incompleteness) | Same concept, different integration point: checks move from ingest_rows match arms to export_* functions |
| R-17 (import ordering violation) | R-17 (skip-set outside DEFERRED snapshot — TOCTOU) | Replaces hand-edited-file ordering concern with snapshot consistency concern. ADR-008 explicitly addresses this. |
| R-18 (default path regression) | R-18 (default path regression) | Same risk, reframed: empty HashSet passed to exporters vs. no HashSet allocated in ingest_rows |
| R-19 (co_access dual-column) | R-19 (co_access dual-column) | Same risk, same 4-combination test matrix, but checks happen at export time |
| R-20 (graph_edges dual-column) | R-20 (graph_edges dual-column) | Same risk, same 4-combination test matrix, but checks happen at export time |
| R-21 (non-entry tables filtered) | R-21 (non-entry tables filtered) | Same risk, reframed for export side |
| R-22 (skip count reporting) | R-22 (export skip count reporting) | Moved from import summary to export summary |
| R-23 (hash validation fails after skip) | R-23 (--confirm safeguard bypass) | **ELIMINATED** old hash interaction risk. Export-side filtering means the hash covers exactly the filtered rows — no mismatch. Replaced with --confirm safeguard bypass risk (ADR-009). |
| R-24 (memory pressure) | R-24 (export header missing skip_quarantined metadata) | Memory concern downgraded to negligible. Replaced with header metadata risk. |

### Key Design Insight
Moving filtering from import to export **eliminates an entire risk category** (hash validation interaction). The old R-23 required either `--skip-hash-validation` or a complex post-filter hash recomputation. With export-side filtering, the footer hash covers exactly the rows in the file — import validates against the same set.

## Artifacts
- `/workspaces/unimatrix/product/features/nxs-012/RISK-TEST-STRATEGY.md` (replaced)

## Summary
- **24 risks** total (15 retained + 9 new)
- **65 test scenarios**
- **10 High priority** risks (R-01, R-02, R-14, R-15, R-16, R-17, R-18, R-19, R-20, R-23)
- **7 Medium priority** risks
- **7 Low priority** risks

## Top Risks for Delivery Attention
1. **R-16 (export cascade incompleteness)** — All 5 entry-referencing exporters must check skip_ids. A missed check produces orphaned rows. Pattern #3910 documents this exact bug class.
2. **R-17 (TOCTOU on skip-set query)** — Skip-set query MUST execute inside the BEGIN DEFERRED transaction. ADR-008 is explicit but implementation must verify.
3. **R-19/R-20 (dual-column checks)** — co_access and graph_edges both have two entry ID columns. Both must be checked. Lesson #4536 confirms this is invisible without explicit per-column tests.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" -- found #4473, #2758, #4177, #1203, #4147 (general gate/test discipline lessons, not directly applicable to export filtering)
- Queried: /uni-knowledge-search for "risk pattern export import filtering cascade" -- found ADR entries #4609, #4613, #4611 (nxs-012 ADRs, used for context)
- Queried: /uni-knowledge-search for "consistent status filtering ghost records" -- found #3910 (multi-pass cleanup pattern, directly applicable to R-16), #4536 (status guard testing lesson, directly applicable to R-19/R-20), #3980 (promotion tick quarantine filter, confirms dual-column pattern)
- Queried: /uni-knowledge-search for "NaN f64 precision" -- found #4133, #4533 (NaN guard patterns, directly applicable to R-01)
- Stored: nothing novel to store -- the export-side filtering risk pattern is feature-specific, not cross-feature. The multi-pass consistency pattern (#3910) already captures the general principle.
