# effectiveness-store Pseudocode

## Purpose

SQL aggregation methods on Store that join injection_log + sessions to produce pre-aggregated data for the effectiveness engine. Two methods: `compute_effectiveness_aggregates()` (4 SQL queries under 1 lock_conn) and `load_entry_classification_meta()` (1 query for entry metadata). Both added to `crates/unimatrix-store/src/read.rs`.

## Structs (added to `crates/unimatrix-store/src/read.rs`)

```
// Import: use std::collections::HashSet;
// Import: use unimatrix_engine::effectiveness::DataWindow;
//   (or define DataWindow in store and re-export from engine -- see note below)

pub struct EffectivenessAggregates {
    pub entry_stats: Vec<EntryInjectionStats>,
    pub active_topics: HashSet<String>,
    pub calibration_rows: Vec<(f64, bool)>,
    pub data_window: DataWindow,
}

pub struct EntryInjectionStats {
    pub entry_id: u64,
    pub injection_count: u32,
    pub success_count: u32,
    pub rework_count: u32,
    pub abandoned_count: u32,
}

pub struct EntryClassificationMeta {
    pub entry_id: u64,
    pub title: String,
    pub topic: String,
    pub trust_source: String,
    pub helpful_count: u32,
    pub unhelpful_count: u32,
}
```

**DataWindow location note**: The architecture defines DataWindow in unimatrix-engine::effectiveness but the store needs to return it. Two options: (a) unimatrix-store depends on unimatrix-engine and uses the engine type, or (b) define a parallel struct in the store. Check existing Cargo.toml -- if unimatrix-store already depends on unimatrix-engine, use option (a). If not, define DataWindow in the store and have the server map it. The implementation brief places DataWindow in the engine types section, so the server can construct it from raw store output.

**Practical approach**: Have `compute_effectiveness_aggregates` return the raw scalars (session_count, earliest, latest) and let the server construct DataWindow from unimatrix-engine. This avoids a store->engine dependency.

Revised struct:
```
pub struct EffectivenessAggregates {
    pub entry_stats: Vec<EntryInjectionStats>,
    pub active_topics: HashSet<String>,
    pub calibration_rows: Vec<(f64, bool)>,
    pub session_count: u32,
    pub earliest_session_at: Option<u64>,
    pub latest_session_at: Option<u64>,
}
```

The server constructs DataWindow from these three fields. This keeps store free of engine dependency.

## Function: `compute_effectiveness_aggregates`

```
impl Store {
    pub fn compute_effectiveness_aggregates(&self) -> Result<EffectivenessAggregates>

        conn = self.lock_conn()

        // -- Query 1: Entry injection stats (ADR-001) --
        // Uses idx_injection_log_entry for GROUP BY, idx_injection_log_session for JOIN
        // R-03: COUNT(DISTINCT il.session_id) prevents duplicate inflation
        stmt = conn.prepare(
            "SELECT il.entry_id,
                    COUNT(DISTINCT il.session_id) as injection_count,
                    COALESCE(SUM(CASE WHEN s.outcome = 'success' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN s.outcome = 'rework' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN s.outcome = 'abandoned' THEN 1 ELSE 0 END), 0)
             FROM injection_log il
             JOIN sessions s ON il.session_id = s.session_id
             WHERE s.outcome IS NOT NULL
             GROUP BY il.entry_id"
        )?

        entry_stats = Vec::new()
        for each row in stmt.query_map([], ...):
            entry_stats.push(EntryInjectionStats {
                entry_id: row.get::<_, i64>(0) as u64,
                injection_count: row.get::<_, i64>(1) as u32,
                success_count: row.get::<_, i64>(2) as u32,
                rework_count: row.get::<_, i64>(3) as u32,
                abandoned_count: row.get::<_, i64>(4) as u32,
            })

        // -- Query 2: Active topics (ADR-002) --
        // Sessions with NULL/empty feature_cycle excluded
        // Uses idx_sessions_feature_cycle
        stmt = conn.prepare(
            "SELECT DISTINCT feature_cycle
             FROM sessions
             WHERE feature_cycle IS NOT NULL AND feature_cycle != ''"
        )?

        active_topics = HashSet::new()
        for each row in stmt.query_map([], ...):
            active_topics.insert(row.get::<_, String>(0))

        // -- Query 3: Calibration rows --
        // Returns one row per injection_log record joined with sessions
        // R-06: Full scan concern for large datasets, but no better approach
        //        without pre-aggregation in SQL (architecture specifies per-row return)
        stmt = conn.prepare(
            "SELECT il.confidence, (s.outcome = 'success') as succeeded
             FROM injection_log il
             JOIN sessions s ON il.session_id = s.session_id
             WHERE s.outcome IS NOT NULL"
        )?

        calibration_rows = Vec::new()
        for each row in stmt.query_map([], ...):
            confidence: f64 = row.get(0)
            succeeded: bool = row.get::<_, i64>(1) != 0
            calibration_rows.push((confidence, succeeded))

        // -- Query 4: Data window --
        // Cheap scalar query
        (session_count, earliest, latest) = conn.query_row(
            "SELECT COUNT(*), MIN(started_at), MAX(started_at)
             FROM sessions
             WHERE outcome IS NOT NULL",
            [],
            |row| {
                count = row.get::<_, i64>(0) as u32
                earliest = row.get::<_, Option<i64>>(1).map(|v| v as u64)
                latest = row.get::<_, Option<i64>>(2).map(|v| v as u64)
                Ok((count, earliest, latest))
            }
        )?

        return Ok(EffectivenessAggregates {
            entry_stats,
            active_topics,
            calibration_rows,
            session_count,
            earliest_session_at: earliest,
            latest_session_at: latest,
        })
```

Key implementation details:
- Single `lock_conn()` call for all 4 queries (ADR-001, R-07: prevents GC race)
- `COUNT(DISTINCT il.session_id)` in Query 1 (R-03: prevents duplicate inflation)
- Query 2 excludes NULL/empty feature_cycle (ADR-002)
- Query 3 returns raw rows, not pre-bucketed (engine does bucketing)
- All map_err(StoreError::Sqlite) on rusqlite errors, following existing pattern

## Function: `load_entry_classification_meta`

```
impl Store {
    pub fn load_entry_classification_meta(&self) -> Result<Vec<EntryClassificationMeta>>

        conn = self.lock_conn()

        // Load metadata for active entries only (status = 0 = Active)
        // ADR-002: COALESCE for NULL/empty topic -> "(unattributed)"
        stmt = conn.prepare(
            "SELECT id, title,
                    CASE WHEN topic IS NULL OR topic = '' THEN '(unattributed)' ELSE topic END,
                    COALESCE(trust_source, ''),
                    helpful_count, unhelpful_count
             FROM entries
             WHERE status = 0"
        )?

        result = Vec::new()
        for each row in stmt.query_map([], ...):
            result.push(EntryClassificationMeta {
                entry_id: row.get::<_, i64>(0) as u64,
                title: row.get::<_, String>(1),
                topic: row.get::<_, String>(2),      // already coalesced by SQL
                trust_source: row.get::<_, String>(3),
                helpful_count: row.get::<_, i64>(4) as u32,
                unhelpful_count: row.get::<_, i64>(5) as u32,
            })

        return Ok(result)
```

Key implementation details:
- Active entries only (status = 0), matching Status::Active enum value
- NULL/empty topic mapped to "(unattributed)" in SQL (ADR-002, AC-16)
- trust_source COALESCE to empty string (consistent with existing patterns)
- Separate lock_conn() from compute_effectiveness_aggregates -- these are called sequentially in Phase 8

## Error Handling

Both methods return `Result<T>` using the existing `StoreError` type:
- `StoreError::Sqlite(rusqlite::Error)` for any query failure
- No custom error variants needed
- Errors propagate to StatusService which catches them and sets `effectiveness = None`

## Key Test Scenarios

1. **Basic aggregation (AC-01)**: Insert entries + injection_log + sessions with known outcomes, verify entry_stats match expected counts
2. **COUNT DISTINCT (R-03)**: Insert 3 injection_log rows for same (entry_id, session_id), verify injection_count = 1
3. **NULL feature_cycle excluded from active_topics (ADR-002, AC-16)**: Insert session with NULL feature_cycle, verify not in active_topics; verify its outcomes still count in entry_stats
4. **Empty tables**: All queries return empty results, no errors
5. **NULL/empty topic in entries (ADR-002)**: Insert entry with NULL topic, verify load_entry_classification_meta returns "(unattributed)"
6. **Sessions with NULL outcome excluded**: Insert session with NULL outcome, verify it does not appear in entry_stats or calibration_rows
7. **Data window correctness**: Insert sessions with known timestamps, verify min/max/count match
8. **Calibration row count**: Insert N injection_log rows with outcomes, verify calibration_rows.len() == N (one per injection, not per distinct session)
9. **Performance (R-06)**: 500 entries, 10K injection_log rows -- should complete under 500ms (may be a benchmark test rather than assertion)

All tests extend existing TestDb helper. Use existing insertion helpers from injection_log.rs and sessions.rs test modules.
