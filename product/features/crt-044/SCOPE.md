# crt-044: Bidirectional S1/S2/S8 Edge Back-fill and graph_expand Security Comment

## Problem Statement

crt-041 writes S1 (tag co-occurrence) and S2 (structural vocabulary) Informs edges using the
`t2.entry_id > t1.entry_id` convention in `graph_enrichment_tick.rs` (S1 line 92, S2 line 196).
This produces a single directed edge per pair: `(lower_id → higher_id)`. crt-041's S8 tick
constructs pairs with `a = min(ids), b = max(ids)` (line 330) and writes only `(a → b)`.

With crt-042's Outgoing-only `graph_expand` traversal, seeds in the higher-ID position cannot
reach their lower-ID S1/S2 Informs partners or lower-ID S8 CoAccess partners. Half the graph
relationship signal is invisible to the expander. The crt-042 SR-03 gate check confirmed
0 bidirectional Informs pairs out of 83 in the live DB snapshot.

This is the hard prerequisite for the crt-042 eval gate (`ppr_expander_enabled`) to produce
meaningful P@5 improvements. Without it, the eval cannot pass.

A secondary finding from the crt-042 security review (Finding 1): `graph_expand` lacks a
visible caller-contract comment at the function signature level, making the quarantine obligation
invisible in an IDE without opening the module header.

**Affected users:** All agents and human queries that use the PPR expander path once
`ppr_expander_enabled = true` becomes the default.

## Goals

1. Back-fill the reverse direction for all existing S1 and S2 Informs edges in `GRAPH_EDGES`
   (schema v19 → v20 migration).
2. Back-fill the reverse direction for all existing S8 CoAccess edges in `GRAPH_EDGES`
   (same v20 migration, scoped to `source = 'S8'`).
3. Update `run_s1_tick` to write both `(source_id, target_id)` and `(target_id, source_id)`
   Informs edges going forward.
4. Update `run_s2_tick` to write both directions going forward.
5. Update `run_s8_tick` to write both directions going forward.
6. Add a `// SECURITY:` caller-contract comment on the `graph_expand` function signature line
   (`graph_expand.rs:89`, now line 68 after crt-042 shipping) making the quarantine obligation
   visible at every IDE usage site.

## Non-Goals

- Any change to `co_access_promotion_tick.rs` or the `co_access`-sourced CoAccess edges — these
  were made bidirectional in crt-035 and are already correct.
- Any change to NLI-origin Informs edges (`source = 'nli'`). NLI writes are
  intentionally unidirectional (source toward neighbor) per col-030 ADR.
- Any change to Cosine Supports edges (`source = 'cosine_supports'`). Directionality of Supports
  edges is out of scope.
- Any change to Supersedes or Contradicts edges. These are directional by design.
- Enabling `ppr_expander_enabled = true` as the default. That is a post-eval decision owned by
  crt-042.
- Running or evaluating the crt-042 eval gate (`run_eval.py`). This feature delivers the
  prerequisite; the eval run is the crt-042 delivery team's responsibility.
- Any change to `graph_expand` traversal logic, BFS depth, or candidate cap.
- Any change to `GRAPH_EDGES` schema columns (no new columns, no UNIQUE constraint changes).
- Deduplicating S1/S2 Informs edges against NLI Informs edges. The existing
  `UNIQUE(source_id, target_id, relation_type)` first-writer-wins semantics are unchanged.

## Background Research

### S1/S2 Single-Direction Root Cause

`run_s1_tick` (`graph_enrichment_tick.rs:92`): `t2.entry_id > t1.entry_id` in the JOIN
produces `source_id = lower_id`, `target_id = higher_id` exclusively.

`run_s2_tick` (`graph_enrichment_tick.rs:196`): `e2.id > e1.id` applies the same convention.

Both ticks call `write_graph_edge(source_id, target_id, ...)` once per pair with the
lower-ID entry as source. Only one direction is written.

### S8 Single-Direction Root Cause

`run_s8_tick` (`graph_enrichment_tick.rs:329-331`): pairs are constructed as
`(entry_ids[i].min(entry_ids[j]), entry_ids[i].max(entry_ids[j]))` — `a < b` always. The single
`write_graph_edge(*a, *b, "CoAccess", ...)` call writes only `(lower → higher)`.

**S8 is NOT currently bidirectional.** It was not fixed by crt-035 because crt-035 only covered
`source = 'co_access'` edges (from `run_co_access_promotion_tick`). S8 edges use
`source = 'S8'` and were not in scope for crt-035's v18→v19 migration.

### crt-035 Precedent (Back-fill Pattern, Entry #3889)

The v18→v19 migration back-filled reverse CoAccess edges for `source = 'co_access'` only.
The template SQL is:

```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
SELECT
    g.target_id          AS source_id,
    g.source_id          AS target_id,
    g.relation_type      AS relation_type,
    g.weight             AS weight,
    strftime('%s','now') AS created_at,
    g.created_by         AS created_by,
    g.source             AS source,
    0                    AS bootstrap_only
FROM graph_edges g
WHERE g.relation_type = 'Informs'         -- or 'CoAccess' for S8
  AND g.source IN ('S1', 'S2')            -- or 'S8' for S8 block
  AND NOT EXISTS (
    SELECT 1 FROM graph_edges rev
    WHERE rev.source_id = g.target_id
      AND rev.target_id = g.source_id
      AND rev.relation_type = g.relation_type
  )
```

Key lesson (entry #3889): filter by `source`, NOT by `created_by`. `created_by` alone misses
tick-era edges. `source` is the correct discriminator.

### Forward Write Pattern (crt-035 / co_access_promotion_tick.rs)

`co_access_promotion_tick.rs` calls `promote_one_direction(a, b)` and then
`promote_one_direction(b, a)` per pair — two calls per pair with swapped arguments.
S1/S2/S8 must follow the same two-call pattern using `write_graph_edge`.

### graph_expand Security Comment

`graph_expand.rs` module header (lines 12-18) contains the quarantine obligation in a doc
block. The function signature at line 68 has no inline `// SECURITY:` comment. The crt-042
security review identified this as Finding 1: the obligation is invisible when hovering over
the function in an IDE. The fix is a single-line comment immediately before or on the `pub fn`
line — documentation-only, no logic change.

### Current Schema Version

`CURRENT_SCHEMA_VERSION = 19` (`migration.rs:19`). This feature adds v20.

### graph_expand.rs Line Reference

After crt-042 delivery, `graph_expand.rs` has the `pub fn graph_expand(` signature at line 68.
The issue references line 89 (pre-crt-042 state); the delivered file is at line 68.

## Proposed Approach

**Phase 1 — Schema migration (v19 → v20):**
Add a `v19 → v20` block in `migration.rs`. Two SQL statements:
1. Back-fill reverse S1+S2 Informs edges: `INSERT OR IGNORE ... SELECT (swap) ... WHERE
   relation_type='Informs' AND source IN ('S1','S2') AND NOT EXISTS(reverse)`.
2. Back-fill reverse S8 CoAccess edges: same pattern with `relation_type='CoAccess' AND
   source='S8'`.

Bump `CURRENT_SCHEMA_VERSION` from 19 to 20.

**Phase 2 — Forward tick writes:**
In `run_s1_tick` and `run_s2_tick`: add a second `write_graph_edge` call per pair with
`source_id` and `target_id` swapped. SQL query shapes do not change.

In `run_s8_tick`: after writing `(*a, *b, "CoAccess", ...)`, add a second call writing
`(*b, *a, "CoAccess", ...)`. The `valid_ids` check and `pairs_written` counter apply to
both directions symmetrically. The `pairs_written` counter should count pairs (not individual
direction writes) or count both edges — this is an open question; see OQ-1.

**Phase 3 — Security comment:**
Add `// SECURITY: caller MUST apply SecurityGateway::is_quarantined() before inserting
returned IDs into result sets.` as a comment on the `pub fn graph_expand(` line in
`graph_expand.rs`.

**Rationale for migration approach:** Follows the proven v18→v19 CoAccess back-fill template
exactly. `INSERT OR IGNORE` idempotency is free via the existing
`UNIQUE(source_id, target_id, relation_type)` constraint. The `NOT EXISTS` guard prevents
double-work on re-runs. Two separate `WHERE source IN (...)` blocks keep S1+S2 scoped
to Informs and S8 scoped to CoAccess.

## Acceptance Criteria

- AC-01: After applying the v19→v20 migration, the query
  `SELECT COUNT(*) FROM GRAPH_EDGES g1 WHERE g1.relation_type='Informs'
  AND EXISTS (SELECT 1 FROM GRAPH_EDGES g2 WHERE g2.source_id=g1.target_id
  AND g2.target_id=g1.source_id AND g2.relation_type='Informs')`
  returns a non-zero count equal to the total Informs edge count (every forward Informs
  edge has a reverse partner).
- AC-02: After applying the v19→v20 migration, the equivalent query for
  `relation_type='CoAccess' AND source='S8'` returns a count equal to the S8 CoAccess
  edge count (every forward S8 CoAccess edge has a reverse partner).
- AC-03: `run_s1_tick` writes two rows per qualifying pair going forward: `(lower_id,
  higher_id, 'Informs', ...)` and `(higher_id, lower_id, 'Informs', ...)`.
- AC-04: `run_s2_tick` writes two rows per qualifying pair going forward (same pattern as AC-03).
- AC-05: `run_s8_tick` writes two rows per qualifying pair going forward: `(*a, *b,
  'CoAccess', ...)` and `(*b, *a, 'CoAccess', ...)`.
- AC-06: `CURRENT_SCHEMA_VERSION` is incremented to 20 in `migration.rs`.
- AC-07: The `v19 → v20` migration block uses `INSERT OR IGNORE` and is idempotent — running
  it twice produces no duplicate rows and no errors.
- AC-08: The `pub fn graph_expand(` line in `graph_expand.rs` carries a `// SECURITY:` comment
  stating the caller quarantine obligation (matching the text in GH#495).
- AC-09: All existing migration tests pass. The new migration block has at least one test
  asserting that a forward-only S1 Informs edge and a forward-only S8 CoAccess edge each
  gain a reverse partner after migration.
- AC-10: All existing `graph_enrichment_tick` tests pass. New tests for `run_s1_tick`,
  `run_s2_tick`, and `run_s8_tick` assert that both `(a→b)` and `(b→a)` edges exist after
  the tick runs on a two-entry fixture.
- AC-11: `cargo test --workspace` passes with no regressions.

## Constraints

- **C-01**: Filter back-fill by `source` field (`'S1'`, `'S2'`, `'S8'`), NOT by `created_by`.
  `created_by` alone misses tick-era edges (entry #3889).
- **C-02**: Use `INSERT OR IGNORE` semantics. The existing
  `UNIQUE(source_id, target_id, relation_type)` constraint in `GRAPH_EDGES` provides
  idempotency — no schema change required.
- **C-03**: S1+S2 are `relation_type='Informs'`; S8 is `relation_type='CoAccess'`. These are
  separate `WHERE` clauses in the migration — do not combine.
- **C-04**: `source = 'nli'` and `source = 'cosine_supports'` Informs edges must NOT be
  back-filled. Filter explicitly.
- **C-05**: The `NOT EXISTS` reverse-edge guard in the migration SQL prevents duplicate
  rows on re-runs but is not required for correctness (INSERT OR IGNORE handles it). Include
  both for defence-in-depth, matching the v18→v19 pattern.
- **C-06**: `run_s8_tick`'s `pairs_written` counter: both direction writes for a single pair
  count as two edges, matching how `run_co_access_promotion_tick` counts (each direction is an
  independent INSERT). If the tick log currently reports per-pair counts, the semantics change
  to per-edge. This is acceptable — see OQ-1.
- **C-07**: `graph_expand.rs` is a pure function — the security comment is documentation only.
  No logic change to `graph_expand` itself.
- **C-08**: The migration transition point is `CURRENT_SCHEMA_VERSION` 19 → 20. The migration
  `migrate_if_needed` function must check `current_version < 20` in the new block.
- **C-09**: `write_graph_edge` is idempotent (`INSERT OR IGNORE`). The second direction call
  per pair in tick functions requires no special handling for already-existing reverse edges.

## Open Questions

- **OQ-1 (RESOLVED)**: `run_s8_tick`'s `pairs_written` counter counts individual edge INSERTs
  (per-edge, not per logical pair). Each direction write is independent, consistent with
  `co_access_promotion_tick`. The counter reflects actual DB writes. Document the count
  semantics change (2× previous values for new pairs) in the PR description.

- **OQ-2 (RESOLVED)**: One combined `WHERE source IN ('S1', 'S2')` statement in the migration.
  Both sources share identical back-fill logic and `relation_type='Informs'`. S8 remains a
  separate statement (`relation_type='CoAccess'`).

- **OQ-3 (RESOLVED — no action)**: AC text references function name, not line number.
  Implementation targets the `pub fn graph_expand(` signature at its current location.

## Tracking

GH Issue: #497 (implementation)
Prerequisite reference: #495

---

*Researched by crt-044-researcher (claude-sonnet-4-6). Written 2026-04-03.*
