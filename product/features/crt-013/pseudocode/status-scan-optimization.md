# Component: Status Scan Optimization

## Purpose

Replace the full entries table scan in `status.rs:136-144` with SQL aggregation queries. New Store methods compute scalar aggregates without deserializing all entries.

## Changes

### 1. Add StatusAggregates struct to unimatrix-store

**File**: `crates/unimatrix-store/src/read.rs` (or new `aggregates.rs`)

```
#[derive(Debug, Clone)]
pub struct StatusAggregates {
    pub supersedes_count: u64,
    pub superseded_by_count: u64,
    pub total_correction_count: u64,
    pub trust_source_distribution: BTreeMap<String, u64>,
    pub unattributed_count: u64,
}
```

### 2. Add compute_status_aggregates() to Store

**File**: `crates/unimatrix-store/src/read.rs`

```
impl Store {
    pub fn compute_status_aggregates(&self) -> Result<StatusAggregates> {
        let conn = self.lock_conn();

        // Query 1: Scalar aggregates (single row)
        let (supersedes_count, superseded_by_count, total_correction_count, unattributed_count) =
            conn.query_row(
                "SELECT
                    COALESCE(SUM(CASE WHEN supersedes IS NOT NULL THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN superseded_by IS NOT NULL THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(correction_count), 0),
                    COALESCE(SUM(CASE WHEN created_by = '' OR created_by IS NULL THEN 1 ELSE 0 END), 0)
                FROM entries",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)? as u64,
                        row.get::<_, i64>(1)? as u64,
                        row.get::<_, i64>(2)? as u64,
                        row.get::<_, i64>(3)? as u64,
                    ))
                },
            ).map_err(StoreError::Sqlite)?;

        // Query 2: Trust source distribution
        let mut trust_source_distribution = BTreeMap::new();
        let mut stmt = conn.prepare(
            "SELECT CASE WHEN trust_source = '' OR trust_source IS NULL
                    THEN '(none)' ELSE trust_source END,
                    COUNT(*)
             FROM entries
             GROUP BY 1"
        ).map_err(StoreError::Sqlite)?;

        let rows = stmt.query_map([], |row| {
            let source: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((source, count as u64))
        }).map_err(StoreError::Sqlite)?;

        for item in rows {
            let (source, count) = item.map_err(StoreError::Sqlite)?;
            trust_source_distribution.insert(source, count);
        }

        Ok(StatusAggregates {
            supersedes_count,
            superseded_by_count,
            total_correction_count,
            trust_source_distribution,
            unattributed_count,
        })
    }
}
```

### 3. Add load_active_entries_with_tags() to Store

```
impl Store {
    pub fn load_active_entries_with_tags(&self) -> Result<Vec<EntryRecord>> {
        let conn = self.lock_conn();

        let mut stmt = conn.prepare(
            &format!("SELECT {} FROM entries WHERE status = 'Active'", ENTRY_COLUMNS)
        ).map_err(StoreError::Sqlite)?;

        let entries: Vec<EntryRecord> = stmt
            .query_map([], entry_from_row)
            .map_err(StoreError::Sqlite)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreError::Sqlite)?;

        // Load tags
        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(&conn, &ids)?;

        let entries_with_tags: Vec<EntryRecord> = entries.into_iter().map(|mut e| {
            if let Some(tags) = tag_map.get(&e.id) {
                e.tags = tags.clone();
            }
            e
        }).collect();

        Ok(entries_with_tags)
    }
}
```

### 4. Replace full scan in StatusService

**File**: `crates/unimatrix-server/src/services/status.rs`

Replace lines 136-182 (the full-table scan block):

```
BEFORE:
  let mut stmt = conn.prepare(&format!("SELECT {} FROM entries", ENTRY_COLUMNS))?;
  let all_entries: Vec<EntryRecord> = ...;
  // ... iterate all_entries to compute aggregates + filter active

AFTER:
  // Aggregates via SQL
  let aggregates = store.compute_status_aggregates()
      .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

  let entries_with_supersedes = aggregates.supersedes_count;
  let entries_with_superseded_by = aggregates.superseded_by_count;
  let total_correction_count = aggregates.total_correction_count;
  let trust_source_dist: BTreeMap<String, u64> = aggregates.trust_source_distribution
      .into_iter().collect();
  let entries_without_attribution = aggregates.unattributed_count;

  // Active entries with tags (for lambda computation + outcome stats)
  let active_entries = store.load_active_entries_with_tags()
      .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
```

The outcome statistics block (lines 184-228) still iterates `all_entries`. This needs adjustment:
- Outcome stats need ALL entries with category="outcome", not just active ones
- Option A: Add `load_outcome_entries_with_tags()` for just outcome entries
- Option B: Load all entries + tags ONLY for outcome computation
- Option C: Compute outcome stats via SQL aggregation too

**Decision**: Keep a targeted query for outcome entries only:
```
  // Outcome entries (small subset)
  let mut outcome_stmt = conn.prepare(
      &format!("SELECT {} FROM entries WHERE category = 'outcome'", ENTRY_COLUMNS)
  )?;
  let outcome_entries: Vec<EntryRecord> = ...;
  let outcome_ids: Vec<u64> = outcome_entries.iter().map(|e| e.id).collect();
  let outcome_tag_map = load_tags_for_entries(&conn, &outcome_ids)?;
  // ... iterate outcome_entries for stats (existing logic)
```

### 5. Exports

**File**: `crates/unimatrix-store/src/lib.rs`

```
ADD to pub use or make accessible:
  pub use read::StatusAggregates;
```

## Error Handling

- SQL errors propagate as `StoreError::Sqlite` (existing pattern)
- Empty database: all counts return 0, empty BTreeMap, empty active_entries vec
- NULL handling in SQL uses CASE expressions matching Rust logic exactly

## Key Test Scenarios

1. compute_status_aggregates() on empty database returns all zeros
2. compute_status_aggregates() counts match manual iteration
3. load_active_entries_with_tags() returns only Active entries
4. load_active_entries_with_tags() includes correct tags
5. Comparison test (AC-10): both paths on same dataset, field-by-field equality
6. Edge case: entry with trust_source="" mapped to "(none)" in both paths
7. Edge case: entry with created_by="" counted as unattributed in both paths
8. Edge case: entry with correction_count = large value
