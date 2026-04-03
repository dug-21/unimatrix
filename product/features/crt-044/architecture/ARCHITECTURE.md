# crt-044: Bidirectional S1/S2/S8 Edge Back-fill and graph_expand Security Comment

## System Overview

crt-041 introduced three graph edge signal sources — S1 (tag co-occurrence), S2 (structural vocabulary), and S8 (search co-retrieval) — all written as single-direction `(lower_id → higher_id)` edges. crt-042 added `graph_expand`, a BFS traversal that follows only Outgoing edges, meaning seeds in the higher-ID position cannot reach their lower-ID partners.

This feature makes S1, S2, and S8 edges fully bidirectional by:

1. Back-filling reverse edges for all existing rows via a schema v19 → v20 migration.
2. Updating the three tick functions to write both directions going forward.
3. Adding a `// SECURITY:` comment at the `graph_expand` function signature to make the quarantine obligation visible at every IDE call site.

This is the hard prerequisite for the crt-042 eval gate (`ppr_expander_enabled`) to produce meaningful P@5 improvements. No changes are made to traversal logic, schema columns, or edge types outside S1/S2/S8.

---

## Component Breakdown

### 1. `migration.rs` — v19 → v20 Back-fill Block

**Location:** `crates/unimatrix-store/src/migration.rs`

**Responsibility:** Insert reverse edges for all existing single-direction S1, S2 (Informs), and S8 (CoAccess) rows. Bump `CURRENT_SCHEMA_VERSION` from 19 to 20.

**Two SQL statements inside `if current_version < 20`:**

Statement A — S1 + S2 Informs reverse edges:

```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at,
     created_by, source, bootstrap_only)
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
WHERE g.relation_type = 'Informs'
  AND g.source IN ('S1', 'S2')
  AND NOT EXISTS (
    SELECT 1 FROM graph_edges rev
    WHERE rev.source_id = g.target_id
      AND rev.target_id = g.source_id
      AND rev.relation_type = 'Informs'
  )
```

Statement B — S8 CoAccess reverse edges:

```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at,
     created_by, source, bootstrap_only)
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
WHERE g.relation_type = 'CoAccess'
  AND g.source = 'S8'
  AND NOT EXISTS (
    SELECT 1 FROM graph_edges rev
    WHERE rev.source_id = g.target_id
      AND rev.target_id = g.source_id
      AND rev.relation_type = 'CoAccess'
  )
```

**Key invariants:**
- `source IN ('S1', 'S2')` and `source = 'S8'` are SEPARATE WHERE clauses because they address different `relation_type` values (`'Informs'` vs `'CoAccess'`). Do not combine.
- `source = 'nli'` and `source = 'cosine_supports'` Informs edges are excluded by the `source IN ('S1', 'S2')` filter. No explicit exclusion clause needed.
- `INSERT OR IGNORE` is the correctness safety net (UNIQUE constraint). `NOT EXISTS` is a defence-in-depth guard that avoids unnecessary IGNORE-discards on re-run, matching the v18→v19 pattern.
- `g.source` is preserved in the inserted row. Reverse S1 edges have `source = 'S1'`; reverse S2 edges have `source = 'S2'`; reverse S8 edges have `source = 'S8'`.
- `bootstrap_only = 0`: reverse edges must be included in live graph traversal (same as v18→v19 pattern).
- Schema version bump: `UPDATE counters SET value = 20 WHERE name = 'schema_version'` inside the `if current_version < 20` block, then the final `INSERT OR REPLACE INTO counters ... CURRENT_SCHEMA_VERSION` at the end of `run_main_migrations` also updated.

### 2. `graph_enrichment_tick.rs` — Forward-Write Bidirectionality

**Location:** `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`

**Responsibility:** After the migration back-fills existing edges, each tick function must write both directions for every new qualifying pair.

**Pattern (from `co_access_promotion_tick.rs`):** Two `write_graph_edge` calls per pair with swapped `source_id`/`target_id` arguments. No changes to the SQL query shape or the `EDGE_SOURCE_*` constants.

**`run_s1_tick` change:**

After the existing `write_graph_edge(store, row.source_id as u64, row.target_id as u64, "Informs", weight, now_ts, EDGE_SOURCE_S1, "").await` call, add:

```rust
if write_graph_edge(
    store,
    row.target_id as u64,
    row.source_id as u64,
    "Informs",
    weight,
    now_ts,
    EDGE_SOURCE_S1,
    "",
).await {
    edges_written += 1;
}
```

Both calls' return values are independently meaningful. `edges_written` is incremented only on `true` return from each call (entry #4041: budget counters MUST key off true only). After migration, the reverse edge likely already exists, so the second call returns `false` via INSERT OR IGNORE — this is correct and expected, not an error.

**`run_s2_tick` change:** Identical pattern to S1, using `row.source_id`/`row.target_id` swapped, `EDGE_SOURCE_S2`, and `"Informs"`.

**`run_s8_tick` change:** After the existing `write_graph_edge(*a, *b, "CoAccess", 0.25_f32, ...)` call inside the `pairs` loop, add:

```rust
if write_graph_edge(
    store,
    *b,
    *a,
    "CoAccess",
    0.25_f32,
    now_ts,
    EDGE_SOURCE_S8,
    "",
).await {
    pairs_written += 1;
}
```

The `valid_ids` check (`if !valid_ids.contains(a) || !valid_ids.contains(b)`) already covers both directions since both IDs are validated before any write. No additional validation needed for the reverse call.

**`pairs_written` counter semantics (SR-01):** The counter now counts individual edge INSERTs (per-edge, not per logical pair). For a new pair `(a, b)` where neither direction exists, `pairs_written` increments by 2. For a pair where one direction already exists (post-migration steady state), it increments by 1. This is consistent with how `co_access_promotion_tick.rs` counts and how `write_graph_edge`'s return value is defined. The tick log field `pairs_written` reflects actual DB writes, not logical pair count. This is a documented semantic change from crt-041's per-pair counting.

### 3. `graph_expand.rs` — Security Comment

**Location:** `crates/unimatrix-engine/src/graph_expand.rs`

**Responsibility:** Add a `// SECURITY:` comment at the `pub fn graph_expand(` signature line (currently line 68) making the quarantine obligation visible in IDE hover and call-site navigation.

The module header (lines 12-18) already contains the full quarantine obligation doc block. The security comment at the signature is a documentation-only change — zero logic change, zero behavioral change. It serves as a call-site obligation marker, following the `// SECURITY:` convention established in `run_s2_tick` (line 155 of `graph_enrichment_tick.rs`).

Comment text:

```rust
// SECURITY: caller MUST apply SecurityGateway::is_quarantined() before inserting
// returned IDs into result sets. graph_expand performs NO quarantine filtering.
pub fn graph_expand(
```

---

## Component Interactions

```
[migration.rs v19→v20]
    └── INSERT OR IGNORE (2 SQL statements)
        ├── GRAPH_EDGES: S1/S2 Informs reverse rows
        └── GRAPH_EDGES: S8 CoAccess reverse rows

[run_s1_tick / run_s2_tick]
    └── write_graph_edge(a→b)  ← existing
    └── write_graph_edge(b→a)  ← new, second call per pair

[run_s8_tick]
    └── write_graph_edge(a, b) ← existing
    └── write_graph_edge(b, a) ← new, second call per pair

[graph_expand (read path)]
    └── edges_of_type(Direction::Outgoing)
        ├── CoAccess: NOW reaches b from a AND a from b (after back-fill)
        └── Informs:  NOW reaches b from a AND a from b (after back-fill)
```

The migration and tick changes are independent: migration fixes the historical debt, tick changes fix the forward path. Both must ship together in crt-044.

---

## Technology Decisions

See ADR-001 (migration strategy), ADR-002 (forward-write pattern), ADR-003 (security comment approach).

---

## Integration Points

| Component | Changed | Nature of Change |
|-----------|---------|-----------------|
| `migration.rs` | Yes | Add `if current_version < 20` block; bump `CURRENT_SCHEMA_VERSION` to 20 |
| `graph_enrichment_tick.rs` | Yes | Add second `write_graph_edge` call in S1, S2, S8 loops |
| `graph_expand.rs` | Yes | Add `// SECURITY:` comment at `pub fn graph_expand(` |
| `GRAPH_EDGES` table | Data only | New reverse-edge rows inserted; no schema column changes |
| `graph_expand` traversal logic | No | Outgoing-only BFS is correct; fix is at write site |
| `co_access_promotion_tick.rs` | No | Already bidirectional (crt-035) |
| NLI Informs edges (`source='nli'`) | No | Intentionally unidirectional per col-030 ADR |
| Cosine Supports edges | No | Out of scope |

---

## Migration Design Detail

### Placement in `run_main_migrations`

The v19→v20 block is appended after the existing `if current_version < 19` block (line 646) and before the final `INSERT OR REPLACE INTO counters` at line 687. The final version bump at line 687 must be updated: `CURRENT_SCHEMA_VERSION` changes from 19 to 20.

The v19→v20 block uses `INSERT OR IGNORE` (not DDL), so it fits cleanly inside the main transaction — no need for an out-of-transaction migration path.

### Idempotency

Two independent layers:
1. `INSERT OR IGNORE` via `UNIQUE(source_id, target_id, relation_type)` — the primary safety net.
2. `NOT EXISTS` sub-query — defence-in-depth to avoid unnecessary ignore-discarded rows on re-run.

Running the block twice produces the same row count as running it once (AC-07).

### Source-Scoped Filtering (C-01, C-04)

- S1/S2 block: `WHERE relation_type = 'Informs' AND source IN ('S1', 'S2')` — excludes `nli`, `cosine_supports`, and any future Informs sources.
- S8 block: `WHERE relation_type = 'CoAccess' AND source = 'S8'` — excludes `co_access` (already fixed by v18→v19) and any future CoAccess sources.
- Filter is by `source`, NOT by `created_by` (entry #3889, C-01).

### write_graph_edge Return Value Contract (SR-02, entry #4041)

`write_graph_edge` returns `bool` via `rows_affected() > 0`. Three cases:
- `true` (rows_affected = 1): new row inserted — increment budget counter.
- `false` (rows_affected = 0, Ok path): UNIQUE conflict — `INSERT OR IGNORE` silently discarded. Expected behavior after migration when the reverse edge already exists. Do NOT warn. Do NOT increment budget counter.
- `false` (Err path): SQL error — `warn!` emitted inside `write_graph_edge`. Do NOT double-log. Do NOT increment budget counter.

The second direction call in each tick function returns `false` for most pairs in a steady-state post-migration DB. This is correct. Implementation agents must not treat this as a bug.

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `write_graph_edge` | `async fn(store: &Store, source_id: u64, target_id: u64, relation_type: &str, weight: f32, created_at: u64, source: &str, metadata: &str) -> bool` | `nli_detection.rs` (re-used) |
| `EDGE_SOURCE_S1` | `&str = "S1"` | `unimatrix-store` constants |
| `EDGE_SOURCE_S2` | `&str = "S2"` | `unimatrix-store` constants |
| `EDGE_SOURCE_S8` | `&str = "S8"` | `unimatrix-store` constants |
| `GRAPH_EDGES` UNIQUE constraint | `UNIQUE(source_id, target_id, relation_type)` | `db.rs` / `migration.rs` v12→v13 |
| `CURRENT_SCHEMA_VERSION` | `pub const u64 = 20` | `migration.rs` (bumped by this feature) |
| `graph_expand` | `pub fn(graph: &TypedRelationGraph, seed_ids: &[u64], depth: usize, max_candidates: usize) -> HashSet<u64>` | `graph_expand.rs:68` |

---

## Test Requirements (from AC-09, AC-10, SR-06)

### Migration Tests (SR-06 per-source regression gate)

Each test asserts bidirectionality for a specific source after migration — three independent test cases:

1. **S1 back-fill test**: Insert forward-only S1 Informs edge `(1→2)`. Run v19→v20 block. Assert `(2→1)` Informs edge exists with `source='S1'`.
2. **S2 back-fill test**: Insert forward-only S2 Informs edge `(3→4)`. Run v19→v20 block. Assert `(4→3)` Informs edge exists with `source='S2'`.
3. **S8 back-fill test**: Insert forward-only S8 CoAccess edge `(5→6)`. Run v19→v20 block. Assert `(6→5)` CoAccess edge exists with `source='S8'`.
4. **Idempotency test**: Run v19→v20 block twice. Assert row count is the same after the second run.
5. **Exclusion test**: Insert `source='co_access'` CoAccess edge and `source='nli'` Informs edge. Run v19→v20 block. Assert these sources' rows are unchanged (no new reverse edges for them).

### Tick Tests (SR-06 per-source regression gate)

Each tick test uses a two-entry fixture and asserts both directions exist after one tick run:

1. **run_s1_tick**: After tick, assert both `(source_id→target_id)` and `(target_id→source_id)` Informs edges with `source='S1'` exist in GRAPH_EDGES.
2. **run_s2_tick**: Same pattern with `source='S2'`.
3. **run_s8_tick**: Same pattern with `source='S8'` and `relation_type='CoAccess'`.

These are per-source integration tests (not just AC coverage) — the intent is to detect future regressions where any one of the three tick functions loses its symmetric second call.

---

## Open Questions

None. All open questions from SCOPE.md are resolved (OQ-1, OQ-2, OQ-3).
