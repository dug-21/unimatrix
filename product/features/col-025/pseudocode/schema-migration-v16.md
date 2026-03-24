# Component: schema-migration-v16

**Crate**: `unimatrix-store`
**Files**: `src/migration.rs`, `src/db.rs`

---

## Purpose

Add `goal TEXT` column to the `cycle_events` table (v15 → v16 migration).
Provide the DB read helper `get_cycle_start_goal` for session resume.
Update `insert_cycle_event` to bind `goal` at the new last parameter position.

---

## New / Modified Functions

### `migration.rs` — `CURRENT_SCHEMA_VERSION`

```
// Change:
pub const CURRENT_SCHEMA_VERSION: u64 = 15;
// To:
pub const CURRENT_SCHEMA_VERSION: u64 = 16;
```

### `migration.rs` — v15 → v16 block inside `migrate_if_needed`

Insert immediately after the existing v14→v15 block (`if current_version < 15 { ... }`).
The schema_version UPDATE at the end of `migrate_if_needed` writes
`CURRENT_SCHEMA_VERSION` (now 16) — no change needed to the UPDATE statement itself.

```
// v15 → v16: cycle_events.goal column (col-025).
if current_version < 16 {
    // Idempotency guard: pattern #1264 — pragma_table_info pre-check.
    // SQLite does not support ALTER TABLE ADD COLUMN IF NOT EXISTS.
    let has_goal_column: bool =
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal'"
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count| count > 0)
        .unwrap_or(false);   // if query fails, treat as absent; ALTER will succeed

    if !has_goal_column {
        sqlx::query("ALTER TABLE cycle_events ADD COLUMN goal TEXT")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }
    // No backfill: pre-existing cycle_events rows get goal = NULL.
    // Existing behavior is unchanged; goal-absent sessions degrade to topic-ID fallback.
}
```

Note: The schema_version UPDATE statement that follows already uses
`CURRENT_SCHEMA_VERSION` (now 16), so it requires no change.

### `db.rs` — `insert_cycle_event` (modified signature)

Current signature (7 params):
```
pub async fn insert_cycle_event(
    &self,
    cycle_id: &str,
    seq: i64,
    event_type: &str,
    phase: Option<&str>,
    outcome: Option<&str>,
    next_phase: Option<&str>,
    timestamp: i64,
) -> Result<()>
```

New signature (8 params — `goal: Option<&str>` added at last position):
```
pub async fn insert_cycle_event(
    &self,
    cycle_id: &str,
    seq: i64,
    event_type: &str,
    phase: Option<&str>,
    outcome: Option<&str>,
    next_phase: Option<&str>,
    timestamp: i64,
    goal: Option<&str>,   // NEW — col-025; None for phase_end and stop events
) -> Result<()>
```

Body changes:
1. Update the SQL string to include `goal` in column list and `?8` in VALUES:
```
"INSERT INTO cycle_events
    (cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
```
2. Add `.bind(goal)` after `.bind(timestamp)`.

Full bind order (positional, must match exactly — R-08 risk):
```
.bind(cycle_id)     // ?1
.bind(seq)          // ?2
.bind(event_type)   // ?3
.bind(phase)        // ?4
.bind(outcome)      // ?5
.bind(next_phase)   // ?6
.bind(timestamp)    // ?7
.bind(goal)         // ?8  NEW
.execute(&mut *conn)
```

Also update `create_tables_if_needed` DDL in `db.rs` to include `goal TEXT`
in the `CREATE TABLE IF NOT EXISTS cycle_events` statement — the two DDLs
must be kept in sync (note in existing v14→v15 code: "DDL mirrors
create_tables_if_needed in db.rs; both must be kept in sync").

### `db.rs` — `get_cycle_start_goal` (new function)

```
/// Load the goal for a cycle_id from the cycle_start event row.
///
/// Returns:
///   Ok(Some(goal)) — cycle_start row exists with non-NULL goal
///   Ok(None)       — row absent, or goal IS NULL (caller omitted goal, or pre-v16 cycle)
///   Err(...)       — DB infrastructure failure (caller degrades to None via unwrap_or_else)
///
/// Uses idx_cycle_events_cycle_id for a single indexed point lookup (pattern #3383).
/// LIMIT 1 guards against duplicate cycle_start rows (defensive, per ADR-001).
pub async fn get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>> {
    let result = sqlx::query_scalar::<_, Option<String>>(
        "SELECT goal FROM cycle_events
         WHERE cycle_id = ?1 AND event_type = 'cycle_start'
         LIMIT 1"
    )
    .bind(cycle_id)
    .fetch_optional(&self.write_pool)   // or read_pool if available; write_pool is safe
    .await
    .map_err(|e| StoreError::Database(e.into()))?;

    // fetch_optional returns:
    //   None      — no row matched (cycle_id absent or event_type != 'cycle_start')
    //   Some(None) — row matched but goal column IS NULL
    //   Some(Some(s)) — row matched and goal is present
    //
    // Flatten: both None and Some(None) return Ok(None) to the caller.
    Ok(result.flatten())
}
```

Implementation note: `sqlx::query_scalar::<_, Option<String>>` returns
`Result<Option<Option<String>>>` from `fetch_optional`. The outer `Option`
represents row presence; the inner `Option` represents NULL vs non-NULL value.
`.flatten()` collapses both absent-row and NULL-goal into `None`.

---

## Initialization Sequence

`migrate_if_needed` is called from `SqlxStore::open()` before any connections
are granted. The v15→v16 block runs inside the existing transaction `txn`
that wraps all migration steps. On idempotent re-run (DB already at v16),
the `if current_version < 16` guard skips the block entirely.

---

## Data Flow

Input: DB at schema v15 (cycle_events table exists without `goal` column)
Output: DB at schema v16 (`goal TEXT` column present, NULL for all pre-v16 rows)

`insert_cycle_event` inputs:
- `goal: Option<&str>` — `Some(text)` for cycle_start with goal; `None` for
  all phase_end and stop events, and for cycle_start without a goal.

`get_cycle_start_goal` inputs: `cycle_id: &str`
`get_cycle_start_goal` outputs: `Result<Option<String>>`

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| `pragma_table_info` query fails | `unwrap_or(false)` — treats column as absent; ALTER TABLE runs |
| `ALTER TABLE` fails (column already exists) | Guarded by idempotency check — should not occur |
| `insert_cycle_event` SQL error | Returns `Err(StoreError::Database(...))` — caller logs warn |
| `get_cycle_start_goal` SQL error | Returns `Err(StoreError::Database(...))` — caller uses `unwrap_or_else` + warn |
| `get_cycle_start_goal` no matching row | Returns `Ok(None)` |
| `get_cycle_start_goal` goal IS NULL | Returns `Ok(None)` |

---

## Migration Test Cascade (SR-01 / R-02)

These existing test files assert a specific schema version and MUST be updated:

1. `tests/migration_v14_to_v15.rs`:
   - `fn test_current_schema_version_is_15` — assert `== 15` must become
     `>= 15` (lower-bound pattern used by v13_to_v14.rs) OR be renamed to
     `test_current_schema_version_is_15_or_later` with `>= 15`.
   - All `assert_eq!(..., 15)` on `CURRENT_SCHEMA_VERSION` must become
     `assert!(... >= 15)`.
   - `read_schema_version` assertions that check `== 15` must become `>= 15`
     (since a fresh DB now initializes to v16).

2. `tests/sqlite_parity.rs` — audit for any `CURRENT_SCHEMA_VERSION == 15`
   or `schema_version == 15` assertions; update to `>= 15` or `== 16`.

3. `tests/sqlite_parity_specialized.rs` — same audit.

New test file to create:
`tests/migration_v15_to_v16.rs` — see Key Test Scenarios below.

---

## Key Test Scenarios

### T-V16-01: Column present after migration (AC-09)
```
setup: open a v15 DB (manually force schema_version = 15 in COUNTERS;
       cycle_events table exists without goal column)
act:   call migrate_if_needed
assert: pragma_table_info('cycle_events') contains row with name = 'goal'
assert: existing rows have goal IS NULL
assert: CURRENT_SCHEMA_VERSION == 16
```

### T-V16-02: Idempotency (AC-09)
```
setup: open a v16 DB (fresh DB after the migration runs)
act:   call migrate_if_needed again
assert: no error
assert: goal column still present (not duplicated)
assert: schema_version still == 16
```

### T-V16-03: insert_cycle_event column binding round-trip (R-08)
```
setup: open a v16 DB
act:   call insert_cycle_event with known values for ALL columns including goal
assert: read the raw row; each column contains its expected value
        (event_type, phase, outcome, next_phase, goal all correct; no transposition)
```

### T-V16-04: insert_cycle_event with None goal (R-08)
```
act:   call insert_cycle_event with goal = None
assert: DB row has goal IS NULL
assert: no other column is displaced
```

### T-V16-05: get_cycle_start_goal happy path (AC-03)
```
setup: insert a cycle_start row with goal = "test goal text"
act:   call get_cycle_start_goal(cycle_id)
assert: returns Ok(Some("test goal text"))
```

### T-V16-06: get_cycle_start_goal absent row (AC-14)
```
act:   call get_cycle_start_goal("nonexistent-cycle-id")
assert: returns Ok(None)
```

### T-V16-07: get_cycle_start_goal NULL goal (AC-03 variant)
```
setup: insert a cycle_start row with goal = NULL (caller omitted goal)
act:   call get_cycle_start_goal(cycle_id)
assert: returns Ok(None)
```

### T-V16-08: get_cycle_start_goal LIMIT 1 semantics (R-10)
```
setup: insert two cycle_start rows for the same cycle_id with different goals
act:   call get_cycle_start_goal(cycle_id)
assert: returns Ok(Some(first_row_goal)) — LIMIT 1 semantics
```
