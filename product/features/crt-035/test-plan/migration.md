# Test Plan: migration_v18_to_v19.rs (crt-035)

**File to create:** `crates/unimatrix-store/tests/migration_v18_to_v19.rs`

---

## Scope

Seven test cases (MIG-U-01 through MIG-U-07) covering the v18→v19 schema migration that
back-fills reverse CoAccess edges in `GRAPH_EDGES`. Follows the pattern of
`tests/migration_v17_to_v18.rs` exactly.

---

## File Header and Shared Infrastructure

```rust
//! Integration tests for the v18→v19 schema migration (crt-035).
//!
//! Covers: MIG-U-01 (CURRENT_SCHEMA_VERSION = 19), MIG-U-02 (fresh DB creates v19),
//! MIG-U-03 (bootstrap-era back-fill), MIG-U-04 (tick-era back-fill),
//! MIG-U-05 (non-CoAccess edges unaffected), MIG-U-06 (idempotency),
//! MIG-U-07 (empty graph_edges no-op).
//!
//! Pattern: create a v18-shaped database (matching post-v17→v18 migration schema),
//! open with current SqlxStore to trigger v18→v19 migration, assert schema state
//! and edge counts.

#![cfg(feature = "test-support")]
```

### `create_v18_database` helper

Must build a database that matches the exact v18 schema — i.e., all tables present after
the v17→v18 migration including `cycle_review_index`. The `graph_edges` table must include:

```sql
CREATE TABLE graph_edges (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id      INTEGER NOT NULL,
    target_id      INTEGER NOT NULL,
    relation_type  TEXT    NOT NULL,
    weight         REAL    NOT NULL DEFAULT 1.0,
    created_at     INTEGER NOT NULL,
    created_by     TEXT    NOT NULL DEFAULT '',
    source         TEXT    NOT NULL DEFAULT '',
    bootstrap_only INTEGER NOT NULL DEFAULT 0,
    metadata       TEXT    DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
)
```

Plus the three separate single-column indexes:
```sql
CREATE INDEX idx_graph_edges_source_id ON graph_edges(source_id);
CREATE INDEX idx_graph_edges_target_id ON graph_edges(target_id);
CREATE INDEX idx_graph_edges_relation_type ON graph_edges(relation_type);
```

And seed `schema_version = 18` in counters.

The helper should mirror the structure of `create_v17_database` in
`tests/migration_v17_to_v18.rs` — all DDL statements, all indexes, then counter seeds.

### Post-migration helpers

```rust
async fn read_schema_version(store: &SqlxStore) -> i64 { ... }

async fn count_graph_edges(
    store: &SqlxStore,
    relation_type: &str,
    source: &str,
) -> i64 { ... }

async fn edge_exists(
    store: &SqlxStore,
    source_id: i64,
    target_id: i64,
    relation_type: &str,
) -> bool { ... }
```

---

## MIG-U-01: CURRENT_SCHEMA_VERSION == 19 (AC-09, R-10)

```rust
#[test]
fn test_current_schema_version_is_19() {
    assert_eq!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION,
        19,
        "CURRENT_SCHEMA_VERSION must be 19"
    );
}
```

**Why:** Catches a missed version bump or a concurrent branch collision. Non-async,
no fixture needed.

---

## MIG-U-02: Fresh DB creates schema v19 (AC-09, R-10)

```rust
#[tokio::test]
async fn test_fresh_db_creates_schema_v19() {
    // Arrange: empty path.
    // Act: SqlxStore::open creates tables at v19 (fresh DB skips migration).
    // Assert: schema_version == 19.
    // Assert: graph_edges table exists with UNIQUE constraint intact.
}
```

**Key assertions:**
- `read_schema_version(&store).await == 19`.
- `SELECT COUNT(*) FROM graph_edges` returns 0 (fresh DB, no data).

---

## MIG-U-03: Bootstrap-era back-fill (AC-06, R-01 — multi-row correctness)

**Scenario:** A v18 DB contains N forward-only CoAccess edges with `created_by='bootstrap'`,
`source='co_access'`. No reverse edges exist. After opening (triggering v18→v19), each
forward edge must have a corresponding reverse edge.

**Arrange:**
```sql
INSERT INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at,
     created_by, source, bootstrap_only)
VALUES
    (1, 2, 'CoAccess', 0.8, 0, 'bootstrap', 'co_access', 0),
    (3, 4, 'CoAccess', 0.6, 0, 'bootstrap', 'co_access', 0),
    (5, 6, 'CoAccess', 1.0, 0, 'bootstrap', 'co_access', 0);
```

**Assert after migration:**
- `count_graph_edges('CoAccess', 'co_access') == 6` (3 forward + 3 reverse).
- `edge_exists(2, 1, 'CoAccess')` is true.
- `edge_exists(4, 3, 'CoAccess')` is true.
- `edge_exists(6, 5, 'CoAccess')` is true.
- Reverse edge for (1→2) has `weight == 0.8`, `created_by == 'bootstrap'`,
  `source == 'co_access'`, `bootstrap_only == 0`.
- `read_schema_version == 19`.

**R-01 coverage:** Use at least 3+ edges (the spec says "1000+" for the volume case, but
3 bootstrap-era edges are sufficient to confirm correctness; the volume path is confirmed
by the EXPLAIN QUERY PLAN gate). The EXPLAIN QUERY PLAN output must be documented in this
file (see GATE-3B-03 in OVERVIEW.md).

**R-04 coverage:** Add one forward edge with `weight = 0.0` in the arrange phase:
```sql
INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at,
     created_by, source, bootstrap_only)
VALUES (7, 8, 'CoAccess', 0.0, 0, 'bootstrap', 'co_access', 0);
```
Assert reverse edge `(8→7)` exists with `weight == 0.0`. This confirms no floor is applied
during back-fill (current behavior; R-04 resolution: no floor).

---

## MIG-U-04: Tick-era back-fill (AC-06)

**Scenario:** A v18 DB contains forward-only CoAccess edges with `created_by='tick'`,
`source='co_access'`. After migration, reverse edges exist with `created_by='tick'`.

**Arrange:**
```sql
INSERT INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at,
     created_by, source, bootstrap_only)
VALUES
    (10, 20, 'CoAccess', 0.5, 0, 'tick', 'co_access', 0),
    (30, 40, 'CoAccess', 0.75, 0, 'tick', 'co_access', 0);
```

**Assert after migration:**
- `count_graph_edges('CoAccess', 'co_access') == 4` (2 forward + 2 reverse).
- `edge_exists(20, 10, 'CoAccess')` is true.
- `edge_exists(40, 30, 'CoAccess')` is true.
- Reverse edge for (10→20) has `created_by == 'tick'` (D1: provenance copied from forward).
- `read_schema_version == 19`.

---

## MIG-U-05: Non-CoAccess edges unaffected (AC-08)

**Scenario:** A v18 DB contains Supersedes, Contradicts, and Supports edges alongside
CoAccess edges. After migration, only CoAccess rows gain reverse edges; the others are
untouched.

**Arrange:**
```sql
-- CoAccess forward edge (will gain a reverse)
INSERT INTO graph_edges (...) VALUES (1, 2, 'CoAccess', 1.0, 0, 'tick', 'co_access', 0);
-- Supersedes edge (must NOT gain a reverse)
INSERT INTO graph_edges (...) VALUES (3, 4, 'Supersedes', 1.0, 0, 'system', 'manual', 0);
-- Contradicts edge (must NOT gain a reverse)
INSERT INTO graph_edges (...) VALUES (5, 6, 'Contradicts', 1.0, 0, 'system', 'manual', 0);
-- Supports edge (must NOT gain a reverse)
INSERT INTO graph_edges (...) VALUES (7, 8, 'Supports', 1.0, 0, 'system', 'manual', 0);
```

**Assert after migration:**
- Total rows in `graph_edges` == 5 (1 original CoAccess + 1 new reverse + 3 non-CoAccess).
- `edge_exists(2, 1, 'CoAccess')` is true.
- `edge_exists(4, 3, 'Supersedes')` is false (no reverse Supersedes written).
- `edge_exists(6, 5, 'Contradicts')` is false.
- `edge_exists(8, 7, 'Supports')` is false.
- Row counts for each non-CoAccess type remain exactly 1.

---

## MIG-U-06: Idempotency — second open does not duplicate reverse edges (AC-07, R-09)

**Scenario:** Open a v18 DB → migration runs (reverse edges inserted, schema bumps to 19).
Close. Open again → migration skipped by version guard (current == 19, not < 19). Edge
count unchanged.

**Arrange:** v18 DB with 2 forward-only CoAccess edges.

**Act:** First open (migration runs). Record edge count. Close. Second open.

**Assert after second open:**
- `read_schema_version == 19` (unchanged).
- `count_graph_edges('CoAccess', 'co_access') == 4` (same as after first open; no duplicates).
- No UNIQUE constraint error on second open.

---

## MIG-U-07: Empty graph_edges at migration time — no-op (EC-01)

**Scenario:** v18 DB with zero rows in `graph_edges`. Migration back-fill SELECT returns
zero rows; INSERT OR IGNORE inserts nothing. Migration completes without error.

**Arrange:** Create v18 DB, do not insert any graph_edges rows.

**Assert after migration:**
- `read_schema_version == 19`.
- `SELECT COUNT(*) FROM graph_edges == 0`.
- No error raised during migration.

---

## GATE-3B-03: EXPLAIN QUERY PLAN Documentation

The test file must include a comment block documenting the EXPLAIN QUERY PLAN output.
Placement: after MIG-U-03 or in a dedicated section at the top of the file. Example:

```rust
// GATE-3B-03: EXPLAIN QUERY PLAN output for back-fill NOT EXISTS sub-join.
//
// Run against a v19-schema tempfile DB:
//   EXPLAIN QUERY PLAN
//   INSERT OR IGNORE INTO graph_edges (...)
//   SELECT g.target_id, g.source_id, ...
//   FROM graph_edges g
//   WHERE g.relation_type = 'CoAccess'
//     AND g.source = 'co_access'
//     AND NOT EXISTS (
//       SELECT 1 FROM graph_edges rev
//       WHERE rev.source_id = g.target_id
//         AND rev.target_id = g.source_id
//         AND rev.relation_type = 'CoAccess'
//     )
//
// Output (captured at delivery):
//   <delivery agent fills in actual output here>
//
// Expected: inner select uses SEARCH via sqlite_autoindex_graph_edges_1 (the UNIQUE
// B-tree on source_id, target_id, relation_type). If SCAN appears for the inner select,
// add a composite covering index to the migration DDL before merging.
```

The delivery agent must run this and replace `<delivery agent fills in actual output here>`
with the real output. If the output shows a full scan, a composite index must be added to
the `if current_version < 19` block in `migration.rs` before merging.

---

## Schema Version Maintenance Note (Pattern #2937)

After bumping `CURRENT_SCHEMA_VERSION` to 19, the delivery agent must search for all
existing tests that hardcode a schema version constant and update them. The known instance
is `test_migration_v7_to_v8_backfill` in `crates/unimatrix-server/src/services/server.rs`
which asserts `version == N` where N is the current version. Failure to update this test
produces a misleading failure: `left=19, right=18`.

---

## Acceptance Criteria Covered by This Plan

| AC-ID | Test(s) |
|-------|---------|
| AC-06 | MIG-U-03, MIG-U-04 |
| AC-07 | MIG-U-06 |
| AC-08 | MIG-U-05 |
| AC-09 | MIG-U-01 |
| AC-10 | All 7 MIG-U cases |
