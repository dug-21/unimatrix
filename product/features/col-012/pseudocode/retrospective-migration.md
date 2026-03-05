# Pseudocode: retrospective-migration

## File: crates/unimatrix-server/src/mcp/tools.rs

### Change: context_retrospective handler

Replace steps 3-5 (observation directory, discover/parse JSONL, attribute) with SqlObservationSource:

```
// Was:
//   3. obs_dir = observation_dir()
//   4. discover_sessions -> parse_session_file -> ParsedSession vec
//   5. attribute_sessions -> attributed records

// Now:
//   3. Create SqlObservationSource
    let source = SqlObservationSource::new(Arc::clone(&self.store))

//   4. Load observations for feature (spawn_blocking for sync I/O)
    let feature_cycle_clone = params.feature_cycle.clone()
    let attributed = tokio::task::spawn_blocking({
        let source = source.clone()  // or move
        move || source.load_feature_observations(&feature_cycle_clone)
    }).await.unwrap()
      .map_err(|e| ServerError::ObservationError(e.to_string()))
      .map_err(rmcp::ErrorData::from)?;

//   5. Check for data availability (same logic as before, using attributed)
    if attributed.is_empty():
        // check cached MetricVector (same as current)
        ...
```

Steps 7-12 remain unchanged -- they receive `Vec<ObservationRecord>` which is the same type.

### Change: Step 9 -- Replace JSONL cleanup with SQL retention

```
// Was:
//   9. Cleanup expired JSONL files

// Now:
//   9. Delete observations older than 60 days
    let store_cleanup = Arc::clone(&self.store)
    tokio::task::spawn_blocking(move || {
        let conn = store_cleanup.lock_conn()
        let sixty_days_millis = 60 * 24 * 60 * 60 * 1000_i64
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .as_millis() as i64 - sixty_days_millis
        conn.execute(
            "DELETE FROM observations WHERE ts_millis < ?1",
            params![cutoff]
        )
    }).await.unwrap();
```

### Change: Step 10e -- Enable narratives (now on SQL path)

```
// Was:
//   report.narratives = None  // JSONL path

// Now:
    report.narratives = Some(unimatrix_observe::synthesize_narratives(&report.hotspots))
```

## File: crates/unimatrix-server/src/services/status.rs

### Change: Phase 6 -- observation stats from SQL

Replace the JSONL-based observation stats (lines 448-466) with:

```
// Was:
//   let obs_dir = observation_dir()
//   let obs_stats = scan_observation_stats(&obs_dir)

// Now:
    let obs_source = SqlObservationSource::new(Arc::clone(&self.store))
    let obs_stats = tokio::task::spawn_blocking(move || {
        obs_source.observation_stats()
    }).await.unwrap()
      .unwrap_or_else(|_| ObservationStats {
          record_count: 0,
          session_count: 0,
          oldest_record_age_days: 0,
          approaching_cleanup: vec![],
      });

    report.observation_record_count = obs_stats.record_count;
    report.observation_session_count = obs_stats.session_count;
    report.observation_oldest_record_days = obs_stats.oldest_record_age_days;
    report.observation_approaching_cleanup = obs_stats.approaching_cleanup;
```

### Change: Phase 4 in run_maintenance -- observation cleanup

Replace JSONL file cleanup (lines 616-624) with SQL retention:

```
// Was:
//   identify_expired(&obs_dir, sixty_days)
//   remove_file(path)

// Now:
    let store_cleanup = Arc::clone(&self.store)
    let _ = tokio::task::spawn_blocking(move || {
        let conn = store_cleanup.lock_conn();
        let sixty_days_millis = 60_i64 * 24 * 60 * 60 * 1000;
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let cutoff = now_millis - sixty_days_millis;
        conn.execute("DELETE FROM observations WHERE ts_millis < ?1", params![cutoff])
    }).await;
```

## File: crates/unimatrix-server/src/mcp/response/status.rs

### Change: StatusReport field names

```
// Rename fields:
//   observation_file_count -> observation_record_count
//   observation_total_size_bytes -> observation_session_count  (type change: u64)
//   observation_oldest_file_days -> observation_oldest_record_days

// Remove:
//   observation_total_size_bytes (not meaningful for SQL)
```

## Notes

- All detection rules unchanged -- they still receive Vec<ObservationRecord>
- MetricVector computation unchanged
- Baseline comparison unchanged
- Report structure unchanged (RetrospectiveReport stays the same)
- R-09: StatusReport field names change -- consumers that ignore unknown fields unaffected
- The SqlObservationSource::load_feature_observations replaces 3 steps:
  discover_sessions + parse_session_file + attribute_sessions
