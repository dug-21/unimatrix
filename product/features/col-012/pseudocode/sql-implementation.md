# Pseudocode: sql-implementation

## NEW File: crates/unimatrix-server/src/services/observation.rs

### SqlObservationSource struct

```rust
use std::sync::Arc;
use unimatrix_store::Store;
use unimatrix_observe::ObservationSource;
use unimatrix_observe::types::{ObservationRecord, ObservationStats, HookType};
use unimatrix_observe::error::Result as ObserveResult;

pub struct SqlObservationSource {
    store: Arc<Store>,
}

impl SqlObservationSource {
    pub fn new(store: Arc<Store>) -> Self {
        SqlObservationSource { store }
    }
}
```

### ObservationSource implementation

```
impl ObservationSource for SqlObservationSource:

    fn load_feature_observations(&self, feature_cycle: &str) -> ObserveResult<Vec<ObservationRecord>>:
        let conn = self.store.lock_conn()

        // Step 1: Get session_ids for this feature from SESSIONS table
        let session_ids = conn.prepare(
            "SELECT session_id FROM sessions WHERE feature_cycle = ?1"
        ).query_map([feature_cycle], |row| row.get::<_, String>(0))

        if session_ids.is_empty():
            return Ok(vec![])

        // Step 2: Query observations for those session_ids
        // Build IN clause with placeholders
        let placeholders = session_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",")
        let sql = format!(
            "SELECT session_id, ts_millis, hook, tool, input, response_size, response_snippet
             FROM observations
             WHERE session_id IN ({})
             ORDER BY ts_millis ASC",
            placeholders
        )

        // Step 3: Map rows to ObservationRecord
        for row in rows:
            let hook_str: String = row.get("hook")
            let hook = match hook_str.as_str():
                "PreToolUse" => HookType::PreToolUse
                "PostToolUse" => HookType::PostToolUse
                "SubagentStart" => HookType::SubagentStart
                "SubagentStop" => HookType::SubagentStop
                _ => continue  // skip unknown hook types

            let input_str: Option<String> = row.get("input")
            let input = match (hook, input_str):
                (SubagentStart, Some(s)) => Some(Value::String(s))   // prompt snippet as String
                (_, Some(s)) => serde_json::from_str(&s).ok()        // tool input as parsed JSON
                (_, None) => None

            let response_size: Option<i64> = row.get("response_size")

            records.push(ObservationRecord {
                ts: row.get::<_, i64>("ts_millis") as u64,
                hook,
                session_id: row.get("session_id"),
                tool: row.get("tool"),
                input,
                response_size: response_size.map(|v| v as u64),
                response_snippet: row.get("response_snippet"),
            })

        Ok(records)  // already sorted by ORDER BY ts_millis ASC

    fn discover_sessions_for_feature(&self, feature_cycle: &str) -> ObserveResult<Vec<String>>:
        let conn = self.store.lock_conn()
        conn.prepare("SELECT session_id FROM sessions WHERE feature_cycle = ?1")
            .query_map([feature_cycle], |row| row.get::<_, String>(0))
            .collect()

    fn observation_stats(&self) -> ObserveResult<ObservationStats>:
        let conn = self.store.lock_conn()

        // Aggregate query
        let (record_count, session_count, min_ts, max_ts) = conn.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT session_id), MIN(ts_millis), MAX(ts_millis)
             FROM observations",
            [], |row| Ok((
                row.get::<_, i64>(0) as u64,
                row.get::<_, i64>(1) as u64,
                row.get::<_, Option<i64>>(2),
                row.get::<_, Option<i64>>(3),
            ))
        )

        let now_millis = SystemTime::now().duration_since(UNIX_EPOCH).as_millis() as i64
        let oldest_record_age_days = match min_ts:
            Some(min) => ((now_millis - min) / 86_400_000).max(0) as u64
            None => 0

        // Sessions approaching cleanup (records 45-59 days old)
        let forty_five_days_millis = 45 * 86_400_000_i64
        let sixty_days_millis = 60 * 86_400_000_i64
        let approaching = conn.prepare(
            "SELECT DISTINCT session_id FROM observations
             WHERE ts_millis < ?1 AND ts_millis >= ?2"
        ).query_map(
            [now_millis - forty_five_days_millis, now_millis - sixty_days_millis],
            // NOTE: reversed -- records older than 45 days but newer than 60 days
            // Actually: approaching_cleanup means 45 <= age < 60 days
            // ts_millis < (now - 45 days) AND ts_millis >= (now - 60 days)
        )

        Ok(ObservationStats {
            record_count,
            session_count,
            oldest_record_age_days,
            approaching_cleanup,
        })
```

## File: crates/unimatrix-server/src/services/mod.rs

### Change: Add observation module

```
pub(crate) mod observation;
```

## Notes

- FR-04.3: Sessions with NULL feature_cycle are excluded (WHERE feature_cycle = ?1 won't match NULL)
- FR-04.4: SubagentStart normalization -- input stored as plain string, returned as Value::String
- R-03: Mapping fidelity -- ts_millis direct (both millis), hook string -> enum parse
- R-05: NULL feature_cycle sessions excluded automatically by SQL WHERE clause
- R-10: Input deserialization -- JSON string for tool inputs, plain String value for SubagentStart
- Error mapping: ObserveError wraps rusqlite errors
