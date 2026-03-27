# Agent Report: col-029-agent-1-pseudocode

Feature: col-029 — Graph Cohesion Metrics in context_status
GH Issue: #413
Stage: 3a (Pseudocode)

---

## Output

| File | Lines | Status |
|------|-------|--------|
| `product/features/col-029/pseudocode/OVERVIEW.md` | ~90 | Done |
| `product/features/col-029/pseudocode/store-cohesion-query.md` | ~200 | Done |
| `product/features/col-029/pseudocode/status-report-fields.md` | ~80 | Done |
| `product/features/col-029/pseudocode/service-call-site.md` | ~110 | Done |
| `product/features/col-029/pseudocode/format-output.md` | ~160 | Done |

---

## Components Covered

1. `store-cohesion-query` — `EDGE_SOURCE_NLI` constant, `GraphCohesionMetrics` struct,
   `compute_graph_cohesion_metrics()` function, `lib.rs` re-export, 8 unit tests
   (7 mandatory AC-13 + 1 empty-store edge case).
2. `status-report-fields` — Six fields appended to `StatusReport` struct and
   hand-written `StatusReport::default()` impl.
3. `service-call-site` — Phase 5 block in `compute_report()`, non-fatal match/warn
   pattern, all six field assignments.
4. `format-output` — Summary conditional one-liner and `#### Graph Cohesion`
   Markdown sub-section inside `### Coherence`.

---

## Key Design Decisions Reflected

- `read_pool()` for all SQL — ADR-003. `write_pool_server()` must not appear.
- Two queries only — ADR-002. UNION sub-query for `connected_entry_count` is
  embedded in Query 2 scalar position to address R-01 (double-count).
- `EDGE_SOURCE_NLI` constant placed near `GraphEdgeRow` definition in `read.rs`,
  re-exported from `lib.rs` — ADR-001.
- Cross-category CASE guard: `ge.id IS NOT NULL AND src_e.category IS NOT NULL AND
  tgt_e.category IS NOT NULL` — ADR-004, R-02.
- Non-fatal error handling at call site mirrors Phase 4 co-access precedent — WARN 1
  resolution from ALIGNMENT-REPORT.md.
- `StatusReport::default()` hand-written — all six fields explicit — R-04.
- `#### Graph Cohesion` (four hashes) not `###` — sub-section of Coherence, not
  peer section. FR-13 spec text used `###` but architecture/brief use `####`.

---

## Open Questions

1. **Connectivity display fraction vs. WAL snapshot**: The Markdown `Connectivity:`
   line displays `(total_active - isolated) / total_active`. `total_active` comes
   from Phase 1 (`compute_status_aggregates`), which runs on `read_pool()` at a
   different moment than Query 2 in `compute_graph_cohesion_metrics()`. Under WAL
   mode, these two snapshots may differ by a few entries. The displayed fraction
   could read `8/9` even if the true counts are `8/10`. This is a cosmetic
   inconsistency accepted as part of the WAL staleness trade-off (ADR-003). No
   structural fix is required; a code comment in `format-output` is sufficient.
   Flagged for implementer awareness.

2. **FR-13 heading level conflict**: SPECIFICATION FR-13 says `### Graph Cohesion`.
   ARCHITECTURE and IMPLEMENTATION BRIEF say `#### Graph Cohesion`. The pseudocode
   uses `####` (four hashes) consistent with the architecture, which places this as
   a sub-section of `### Coherence`. Tester should verify the heading depth matches
   what the implementer chose and that it renders correctly nested.

3. **Summary conditional guards only three of six metrics**: The condition
   `isolated > 0 || cross_category > 0 || inferred > 0` suppresses the Summary line
   when only `supports_edge_count > 0` (and others are zero). R-10 notes this edge
   case is unlikely in practice. The Markdown sub-section is always shown, so
   operators are never completely blind. If the delivery team considers this a gap,
   the condition could be widened to `any of six > 0` — but the brief specifies the
   three-field condition explicitly.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `graph cohesion SQL aggregates store layer patterns` — found #726 (SQL Aggregation Struct pattern), #1588 (Active-only query gotcha). Both applied.
- Queried: `/uni-query-patterns` for `col-029 architectural decisions` (category: decision, topic: col-029) — found #3591 (ADR-001), #3592 (ADR-002), #3594 (ADR-004), #3595 (ADR-003). All four ADRs confirmed stored and consistent with the ADR files read on disk.
- Deviations from established patterns: none. All four components follow established patterns: `compute_status_aggregates` (store layer), field-append pattern (struct), Phase 4 co-access non-fatal handling (service call site), push_str + format! (format output).
