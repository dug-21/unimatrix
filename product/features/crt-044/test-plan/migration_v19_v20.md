# Test Plan: migration_v19_v20

## Component

`crates/unimatrix-store/src/migration.rs` — v19→v20 back-fill block

## Test File

New file: `crates/unimatrix-store/tests/migration_v19_v20.rs`

Pattern: identical to `migration_v18_to_v19.rs`. Each test creates a v19-shaped database using a
`create_v19_database()` helper, inserts fixture rows directly via `sqlx::query`, opens via
`SqlxStore::open()` to trigger migration, then asserts via direct SQL queries.

## Fixture Helper

```rust
// create_v19_database(path: &Path) async
// -- Schema identical to v18 shape (all same tables + cycle_review_index from v17→v18).
// -- Seed counters with schema_version = 19.
// -- graph_edges table present with no rows; callers insert as needed.
```

The v19 schema shape is the v18 shape verbatim (same tables, same indexes, same counter seeds
except `schema_version = 19`). Copy `create_v18_database` from `migration_v18_to_v19.rs`,
rename to `create_v19_database`, and update the schema_version seed value.

## Shared Helpers

Carry over from v18→v19 test file (can be copy-paste + adjust):

```rust
async fn read_schema_version(store: &SqlxStore) -> i64
async fn count_graph_edges(store: &SqlxStore, relation_type: &str, source: &str) -> i64
async fn edge_exists(store: &SqlxStore, source_id: i64, target_id: i64, relation_type: &str) -> bool
```

---

## Test Cases

### MIG-V20-U-01: CURRENT_SCHEMA_VERSION == 20 (AC-06, R-10)

```rust
#[test]
fn test_current_schema_version_is_20()
```

**Arrange**: None — no database needed.

**Act**: Read `unimatrix_store::migration::CURRENT_SCHEMA_VERSION`.

**Assert**:
```rust
assert_eq!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION, 20,
    "CURRENT_SCHEMA_VERSION must be 20");
```

**Risks covered**: R-10 (version constant not bumped).

---

### MIG-V20-U-02: Fresh DB creates schema v20 (R-10)

```rust
#[tokio::test]
async fn test_fresh_db_creates_schema_v20()
```

**Arrange**: Empty tempfile path.

**Act**: `SqlxStore::open()` on fresh path.

**Assert**:
```rust
assert_eq!(read_schema_version(&store).await, 20,
    "fresh database must be at schema v20");
// graph_edges table must exist with no rows.
let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")...;
assert_eq!(row_count, 0);
```

**Risks covered**: R-10 (migration block never runs on fresh DB).

---

### MIG-V20-U-03: S1 Informs edge back-filled (AC-09, AC-01, R-01)

```rust
#[tokio::test]
async fn test_v19_to_v20_back_fills_s1_informs_edge()
```

**Arrange**: v19 database. Insert one forward-only S1 Informs edge:
```sql
INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at,
    created_by, source, bootstrap_only)
VALUES (1, 2, 'Informs', 0.3, 0, 'tick', 'S1', 0)
```

**Assert pre-migration**: Reverse `(2→1, 'Informs')` does NOT exist.

**Act**: `SqlxStore::open()` triggers v19→v20 migration.

**Assert post-migration**:
```rust
assert_eq!(read_schema_version(&store).await, 20);
assert!(edge_exists(&store, 2, 1, "Informs").await,
    "reverse (2→1) S1 Informs edge must be back-filled");
assert!(edge_exists(&store, 1, 2, "Informs").await,
    "forward (1→2) S1 Informs edge must still exist");
// Verify source field copied faithfully.
let row = fetch_edge(&store, 2, 1, "Informs").await.unwrap();
assert_eq!(row.source, "S1", "back-filled row must carry source='S1'");
assert_eq!(row.bootstrap_only, 0);
```

**Risks covered**: R-01 (wrong relation_type in Statement A), AC-09 per-source S1 assertion.

---

### MIG-V20-U-04: S2 Informs edge back-filled (R-01, AC-09)

```rust
#[tokio::test]
async fn test_v19_to_v20_back_fills_s2_informs_edge()
```

**Arrange**: v19 database. Insert one forward-only S2 Informs edge:
```sql
INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at,
    created_by, source, bootstrap_only)
VALUES (3, 4, 'Informs', 0.5, 0, 'tick', 'S2', 0)
```

**Act**: `SqlxStore::open()`.

**Assert**:
```rust
assert!(edge_exists(&store, 4, 3, "Informs").await,
    "reverse (4→3) S2 Informs edge must be back-filled");
let row = fetch_edge(&store, 4, 3, "Informs").await.unwrap();
assert_eq!(row.source, "S2", "back-filled row must carry source='S2'");
```

**Risks covered**: R-01 (S2 branch of Statement A tested independently from S1 — per-source
regression guard per SR-06).

---

### MIG-V20-U-05: S8 CoAccess edge back-filled (AC-09, AC-02, R-01)

```rust
#[tokio::test]
async fn test_v19_to_v20_back_fills_s8_coaccess_edge()
```

**Arrange**: v19 database. Insert one forward-only S8 CoAccess edge:
```sql
INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at,
    created_by, source, bootstrap_only)
VALUES (5, 6, 'CoAccess', 0.25, 0, 'tick', 'S8', 0)
```

**Act**: `SqlxStore::open()`.

**Assert**:
```rust
assert!(edge_exists(&store, 6, 5, "CoAccess").await,
    "reverse (6→5) S8 CoAccess edge must be back-filled");
let row = fetch_edge(&store, 6, 5, "CoAccess").await.unwrap();
assert_eq!(row.source, "S8", "back-filled row must carry source='S8'");
assert_eq!(row.bootstrap_only, 0);
```

**Risks covered**: R-01 (Statement B uses `relation_type='CoAccess'`, not `'Informs'`), AC-09
per-source S8 assertion.

---

### MIG-V20-U-06: Count parity — S1+S2 Informs (AC-01)

```rust
#[tokio::test]
async fn test_v19_to_v20_s1_s2_count_parity_after_migration()
```

**Arrange**: v19 database. Insert two forward-only S1 Informs edges and one S2 Informs edge
(3 forward edges total across both sources).

**Act**: `SqlxStore::open()`.

**Assert**: Total `source IN ('S1','S2')` Informs edges == 6 (3 forward + 3 reverse). Count of
edges with a reverse partner equals total edge count:

```rust
// Every forward edge has a reverse partner.
let total: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM graph_edges
     WHERE relation_type = 'Informs' AND source IN ('S1', 'S2')"
).fetch_one(...).await?;

let paired: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM graph_edges g1
     WHERE g1.relation_type = 'Informs'
       AND g1.source IN ('S1', 'S2')
       AND EXISTS (
         SELECT 1 FROM graph_edges g2
         WHERE g2.source_id = g1.target_id
           AND g2.target_id = g1.source_id
           AND g2.relation_type = 'Informs'
       )"
).fetch_one(...).await?;

assert!(total > 0);
assert_eq!(total, paired, "every S1/S2 Informs edge must have a reverse partner");
```

**Risks covered**: AC-01 full verification query.

---

### MIG-V20-U-07: Count parity — S8 CoAccess (AC-02)

```rust
#[tokio::test]
async fn test_v19_to_v20_s8_count_parity_after_migration()
```

**Arrange**: v19 database. Insert two forward-only S8 CoAccess edges.

**Act**: `SqlxStore::open()`.

**Assert**: Equivalent count parity query scoped to `relation_type='CoAccess' AND source='S8'` on
both `g1` and `g2`. `total == paired > 0`.

**Risks covered**: AC-02.

---

### MIG-V20-U-08: Excluded sources not back-filled (R-06, R-07, AC-09)

```rust
#[tokio::test]
async fn test_v19_to_v20_excludes_excluded_sources()
```

**Arrange**: v19 database. Insert four forward edges that must NOT be back-filled:

```sql
-- nli Informs: intentionally unidirectional
INSERT INTO graph_edges VALUES (10, 11, 'Informs', 1.0, 0, 'nli', 'nli', 0)
-- cosine_supports Informs: out of scope
INSERT INTO graph_edges VALUES (12, 13, 'Informs', 0.8, 0, 'system', 'cosine_supports', 0)
-- co_access CoAccess: already bidirectional since v18→v19
INSERT INTO graph_edges VALUES (14, 15, 'CoAccess', 0.5, 0, 'tick', 'co_access', 0)
INSERT INTO graph_edges VALUES (15, 14, 'CoAccess', 0.5, 0, 'tick', 'co_access', 0)  -- reverse already exists
```

**Act**: `SqlxStore::open()`.

**Assert**:
```rust
// nli reverse must NOT exist.
assert!(!edge_exists(&store, 11, 10, "Informs").await,
    "nli Informs reverse must NOT be back-filled (C-04)");

// cosine_supports reverse must NOT exist.
assert!(!edge_exists(&store, 13, 12, "Informs").await,
    "cosine_supports Informs reverse must NOT be back-filled (C-04)");

// co_access count unchanged (was already 2 — both directions present).
let ca_count = count_graph_edges(&store, "CoAccess", "co_access").await;
assert_eq!(ca_count, 2, "co_access edges must not gain additional rows (R-06)");
```

**Risks covered**: R-06 (co_access accidentally back-filled), R-07 (nli/cosine_supports
accidentally back-filled), C-04.

---

### MIG-V20-U-09: Idempotency — clean state (AC-07, R-09)

```rust
#[tokio::test]
async fn test_v19_to_v20_migration_idempotent_clean_state()
```

**Arrange**: v19 database with two forward-only S1 Informs edges and one S8 CoAccess edge.

**Act**: Open store twice (second `SqlxStore::open()` on same path after the first closes).

**Assert after first open**:
```rust
let count_after_first = total_graph_edges_count(&store).await;  // e.g., 6
assert_eq!(read_schema_version(&store).await, 20);
```

**Assert after second open**:
```rust
let count_after_second = total_graph_edges_count(&store).await;
assert_eq!(count_after_second, count_after_first,
    "second open must not add rows — idempotency guaranteed by INSERT OR IGNORE + NOT EXISTS");
assert_eq!(read_schema_version(&store).await, 20);
```

**Risks covered**: AC-07, R-09 (migration block runs twice due to crash/restart).

---

### MIG-V20-U-10: Idempotency — with pre-existing reverse edge (AC-14, R-09)

```rust
#[tokio::test]
async fn test_v19_to_v20_migration_idempotent_with_preexisting_reverse()
```

This test is **distinct from MIG-V20-U-09** — it exercises partial-bidirectionality input state
(some pairs already bidirectional before migration runs).

**Arrange**: v19 database with:
- Forward-only S1 Informs edge `(1→2)` — no reverse yet
- Pre-existing bidirectional S1 Informs pair `(3→4)` + `(4→3)` — reverse already exists

**Act**: `SqlxStore::open()` (first open, triggers migration).

**Assert**:
```rust
// (1→2) pair gained its reverse.
assert!(edge_exists(&store, 2, 1, "Informs").await);
// (3→4) pair: still 2 rows, no duplicate.
let pairs_34_count: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM graph_edges
     WHERE (source_id = 3 AND target_id = 4 OR source_id = 4 AND target_id = 3)
       AND relation_type = 'Informs'"
).fetch_one(...).await?;
assert_eq!(pairs_34_count, 2, "pre-existing bidirectional pair must remain exactly 2 rows");
```

**Act (second open)**: Close store, reopen same path.

**Assert**: Total row count unchanged after second open.

**Risks covered**: AC-14 (SR-05 idempotency on partial-bidirectionality input).

---

### MIG-V20-U-11: Empty graph_edges — no-op (edge case)

```rust
#[tokio::test]
async fn test_v19_to_v20_empty_graph_edges_is_noop()
```

**Arrange**: v19 database with no graph_edges rows.

**Act**: `SqlxStore::open()`.

**Assert**:
```rust
assert_eq!(read_schema_version(&store).await, 20);
let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")...;
assert_eq!(total, 0, "back-fill on empty table must be a no-op");
```

**Risks covered**: Edge case from RISK-TEST-STRATEGY.md (empty GRAPH_EDGES table).

---

## Risks NOT Covered by Unit Tests

| Risk | Reason | Mitigation |
|------|--------|------------|
| R-02 (crt-043 ships first, consuming v20) | Delivery sequencing — untestable in unit tests | Pre-merge gate: reviewer confirms `CURRENT_SCHEMA_VERSION == 19` in base branch. See IMPLEMENTATION-BRIEF §Delivery Notes. |
| R-09 (transaction boundary) | SQLite in-process tests cannot easily simulate mid-statement failure | Code review confirms both SQL statements are inside the `if current_version < 20` block within `migrate_if_needed`'s outer transaction. Idempotency tests (MIG-V20-U-09, MIG-V20-U-10) provide indirect coverage. |

---

## Test Count Summary

| Test ID | Function | AC | Risk |
|---------|----------|-----|------|
| MIG-V20-U-01 | `test_current_schema_version_is_20` | AC-06 | R-10 |
| MIG-V20-U-02 | `test_fresh_db_creates_schema_v20` | — | R-10 |
| MIG-V20-U-03 | `test_v19_to_v20_back_fills_s1_informs_edge` | AC-09 | R-01 |
| MIG-V20-U-04 | `test_v19_to_v20_back_fills_s2_informs_edge` | AC-09 | R-01 |
| MIG-V20-U-05 | `test_v19_to_v20_back_fills_s8_coaccess_edge` | AC-09 | R-01 |
| MIG-V20-U-06 | `test_v19_to_v20_s1_s2_count_parity_after_migration` | AC-01 | — |
| MIG-V20-U-07 | `test_v19_to_v20_s8_count_parity_after_migration` | AC-02 | — |
| MIG-V20-U-08 | `test_v19_to_v20_excludes_excluded_sources` | AC-09 | R-06, R-07 |
| MIG-V20-U-09 | `test_v19_to_v20_migration_idempotent_clean_state` | AC-07 | R-09 |
| MIG-V20-U-10 | `test_v19_to_v20_migration_idempotent_with_preexisting_reverse` | AC-14 | R-09, R-05 |
| MIG-V20-U-11 | `test_v19_to_v20_empty_graph_edges_is_noop` | — | edge case |

**Total: 11 new tests** (1 non-async + 10 `#[tokio::test]`).

---

*Authored by crt-044-agent-2-testplan (claude-sonnet-4-6). Written 2026-04-03.*
