# Component: AnalyticsQueue + AnalyticsWrite
## File: `crates/unimatrix-store/src/analytics.rs` (new file)

---

## Purpose

Defines the `AnalyticsWrite` enum (one variant per analytics write operation) and the
drain task function. The `SqlxStore` struct in `db.rs` holds the sender half of the
bounded mpsc channel and the `AtomicU64` shed counter. The drain task holds the
receiver and processes batches against `write_pool`.

This file does NOT define `SqlxStore` — it only defines the types and task function that
`SqlxStore` uses.

---

## OQ-DURING-03: AnalyticsWrite Field Completeness Verification

Field sets below are cross-referenced against the `create_tables()` DDL in
`crates/unimatrix-store/src/db.rs` (schema v12). Any deviation is a compile-time
error via `sqlx::query!()` macros in the drain task match arms.

---

## AnalyticsWrite Enum — Verified Against Schema v12

```rust
/// All analytics write operations routed through the bounded channel.
/// Integrity tables (entries, entry_tags, audit_log, agent_registry, vector_map, counters)
/// are NEVER represented here — they always go through write_pool directly.
///
/// #[non_exhaustive]: Wave 1+ variants (GraphEdge, ConfidenceWeightUpdate) are added
/// without breaking the drain task match in dependent crates (FR-17, C-08).
/// External crate match on AnalyticsWrite MUST include a `_ => {}` catch-all arm.
#[non_exhaustive]
pub enum AnalyticsWrite {
    /// Table: co_access (entry_id_a < entry_id_b enforced by CHECK constraint)
    /// Columns: entry_id_a, entry_id_b, count (default 1), last_updated
    /// SQL: INSERT OR IGNORE ... ON CONFLICT update count + last_updated
    CoAccess {
        id_a: u64,    // maps to entry_id_a INTEGER
        id_b: u64,    // maps to entry_id_b INTEGER; caller must ensure id_a < id_b
    },

    /// Table: sessions
    /// All columns from sessions DDL (schema v12, including keywords column from v11→v12 migration)
    SessionUpdate {
        session_id:       String,         // TEXT PRIMARY KEY
        feature_cycle:    Option<String>, // TEXT nullable
        agent_role:       Option<String>, // TEXT nullable
        started_at:       i64,            // INTEGER NOT NULL
        ended_at:         Option<i64>,    // INTEGER nullable
        status:           i64,            // INTEGER NOT NULL DEFAULT 0
        compaction_count: i64,            // INTEGER NOT NULL DEFAULT 0
        outcome:          Option<String>, // TEXT nullable
        total_injections: i64,            // INTEGER NOT NULL DEFAULT 0
        keywords:         Option<String>, // TEXT nullable (v12 column)
    },

    /// Table: injection_log
    /// Columns: log_id (AUTOINCREMENT, not set by caller), session_id, entry_id, confidence, timestamp
    InjectionLog {
        session_id: String, // TEXT NOT NULL
        entry_id:   u64,    // INTEGER NOT NULL
        confidence: f64,    // REAL NOT NULL
        timestamp:  i64,    // INTEGER NOT NULL
    },

    /// Table: query_log
    /// Columns: query_id (AUTOINCREMENT), session_id, query_text, ts, result_count,
    ///          result_entry_ids, similarity_scores, retrieval_mode, source
    QueryLog {
        session_id:        String,         // TEXT NOT NULL
        query_text:        String,         // TEXT NOT NULL
        ts:                i64,            // INTEGER NOT NULL
        result_count:      i64,            // INTEGER NOT NULL
        result_entry_ids:  Option<String>, // TEXT nullable (JSON array)
        similarity_scores: Option<String>, // TEXT nullable (JSON array)
        retrieval_mode:    Option<String>, // TEXT nullable
        source:            String,         // TEXT NOT NULL
    },

    /// Table: signal_queue
    /// Columns: signal_id (AUTOINCREMENT), session_id, created_at, entry_ids, signal_type, signal_source
    SignalQueue {
        session_id:    String, // TEXT NOT NULL
        created_at:    i64,    // INTEGER NOT NULL
        entry_ids:     String, // TEXT NOT NULL DEFAULT '[]' (JSON array)
        signal_type:   i64,    // INTEGER NOT NULL
        signal_source: i64,    // INTEGER NOT NULL
    },

    /// Table: observations
    /// Columns: id (AUTOINCREMENT), session_id, ts_millis, hook, tool, input,
    ///          response_size, response_snippet, topic_signal (v10 column)
    Observation {
        session_id:       String,         // TEXT NOT NULL
        ts_millis:        i64,            // INTEGER NOT NULL
        hook:             String,         // TEXT NOT NULL
        tool:             Option<String>, // TEXT nullable
        input:            Option<String>, // TEXT nullable
        response_size:    Option<i64>,    // INTEGER nullable
        response_snippet: Option<String>, // TEXT nullable
        topic_signal:     Option<String>, // TEXT nullable (v10 column)
    },

    /// Table: observation_metrics
    /// All 23 columns from DDL (PRIMARY KEY = feature_cycle; use INSERT OR REPLACE for upsert)
    ObservationMetric {
        feature_cycle:                      String, // TEXT PRIMARY KEY
        computed_at:                        i64,    // INTEGER NOT NULL DEFAULT 0
        total_tool_calls:                   i64,    // INTEGER NOT NULL DEFAULT 0
        total_duration_secs:                i64,    // INTEGER NOT NULL DEFAULT 0
        session_count:                      i64,    // INTEGER NOT NULL DEFAULT 0
        search_miss_rate:                   f64,    // REAL NOT NULL DEFAULT 0.0
        edit_bloat_total_kb:                f64,    // REAL NOT NULL DEFAULT 0.0
        edit_bloat_ratio:                   f64,    // REAL NOT NULL DEFAULT 0.0
        permission_friction_events:         i64,    // INTEGER NOT NULL DEFAULT 0
        bash_for_search_count:              i64,    // INTEGER NOT NULL DEFAULT 0
        cold_restart_events:                i64,    // INTEGER NOT NULL DEFAULT 0
        coordinator_respawn_count:          i64,    // INTEGER NOT NULL DEFAULT 0
        parallel_call_rate:                 f64,    // REAL NOT NULL DEFAULT 0.0
        context_load_before_first_write_kb: f64,    // REAL NOT NULL DEFAULT 0.0
        total_context_loaded_kb:            f64,    // REAL NOT NULL DEFAULT 0.0
        post_completion_work_pct:           f64,    // REAL NOT NULL DEFAULT 0.0
        follow_up_issues_created:           i64,    // INTEGER NOT NULL DEFAULT 0
        knowledge_entries_stored:           i64,    // INTEGER NOT NULL DEFAULT 0
        sleep_workaround_count:             i64,    // INTEGER NOT NULL DEFAULT 0
        agent_hotspot_count:                i64,    // INTEGER NOT NULL DEFAULT 0
        friction_hotspot_count:             i64,    // INTEGER NOT NULL DEFAULT 0
        session_hotspot_count:              i64,    // INTEGER NOT NULL DEFAULT 0
        scope_hotspot_count:                i64,    // INTEGER NOT NULL DEFAULT 0
    },

    /// Table: shadow_evaluations
    /// Columns: id (AUTOINCREMENT), timestamp, rule_name, rule_category, neural_category,
    ///          neural_confidence, convention_score, rule_accepted, digest (BLOB nullable)
    ShadowEvaluation {
        timestamp:          i64,           // INTEGER NOT NULL
        rule_name:          String,        // TEXT NOT NULL
        rule_category:      String,        // TEXT NOT NULL
        neural_category:    String,        // TEXT NOT NULL
        neural_confidence:  f64,           // REAL NOT NULL
        convention_score:   f64,           // REAL NOT NULL
        rule_accepted:      i64,           // INTEGER NOT NULL
        digest:             Option<Vec<u8>>, // BLOB nullable
    },

    /// Table: feature_entries
    /// Columns: feature_id TEXT NOT NULL, entry_id INTEGER NOT NULL (PRIMARY KEY composite)
    /// SQL: INSERT OR IGNORE (idempotent)
    FeatureEntry {
        feature_id: String, // TEXT NOT NULL
        entry_id:   u64,    // INTEGER NOT NULL
    },

    /// Table: topic_deliveries
    /// All columns from DDL (PRIMARY KEY = topic; use INSERT OR REPLACE for upsert)
    TopicDelivery {
        topic:               String,         // TEXT PRIMARY KEY
        created_at:          i64,            // INTEGER NOT NULL
        completed_at:        Option<i64>,    // INTEGER nullable
        status:              String,         // TEXT NOT NULL DEFAULT 'active'
        github_issue:        Option<i64>,    // INTEGER nullable
        total_sessions:      i64,            // INTEGER NOT NULL DEFAULT 0
        total_tool_calls:    i64,            // INTEGER NOT NULL DEFAULT 0
        total_duration_secs: i64,            // INTEGER NOT NULL DEFAULT 0
        phases_completed:    Option<String>, // TEXT nullable
    },

    /// Table: outcome_index
    /// Columns: feature_cycle TEXT NOT NULL, entry_id INTEGER NOT NULL (PRIMARY KEY composite)
    /// SQL: INSERT OR IGNORE (idempotent)
    OutcomeIndex {
        feature_cycle: String, // TEXT NOT NULL
        entry_id:      u64,    // INTEGER NOT NULL
    },

    // Future Wave 1 variants (not defined here):
    // GraphEdge { ... }             -- W1-1 NLI graph edges
    // ConfidenceWeightUpdate { ... } -- W3-1
}
```

### `variant_name` helper method

```rust
impl AnalyticsWrite {
    /// Returns the variant name as a &'static str for WARN log messages on shed events.
    /// Used by SqlxStore::enqueue_analytics() when try_send returns TrySendError::Full.
    pub(crate) fn variant_name(&self) -> &'static str {
        match self {
            AnalyticsWrite::CoAccess { .. }           => "CoAccess",
            AnalyticsWrite::SessionUpdate { .. }      => "SessionUpdate",
            AnalyticsWrite::InjectionLog { .. }       => "InjectionLog",
            AnalyticsWrite::QueryLog { .. }           => "QueryLog",
            AnalyticsWrite::SignalQueue { .. }        => "SignalQueue",
            AnalyticsWrite::Observation { .. }        => "Observation",
            AnalyticsWrite::ObservationMetric { .. }  => "ObservationMetric",
            AnalyticsWrite::ShadowEvaluation { .. }   => "ShadowEvaluation",
            AnalyticsWrite::FeatureEntry { .. }       => "FeatureEntry",
            AnalyticsWrite::TopicDelivery { .. }      => "TopicDelivery",
            AnalyticsWrite::OutcomeIndex { .. }       => "OutcomeIndex",
            _ => "Unknown",  // catch-all for future non_exhaustive variants
        }
    }
}
```

---

## Constants (defined in this file or pool_config.rs — choose one location)

```rust
pub const ANALYTICS_QUEUE_CAPACITY: usize = 1000;
pub(crate) const DRAIN_BATCH_SIZE: usize = 50;
pub(crate) const DRAIN_FLUSH_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const DRAIN_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
```

---

## Drain Task Function

```rust
/// Long-lived tokio task. Started in SqlxStore::open(), runs for the lifetime of the store.
/// Owned exclusively by SqlxStore; not exported outside the crate.
pub(crate) async fn run_drain_task(
    mut rx: mpsc::Receiver<AnalyticsWrite>,
    mut shutdown_rx: oneshot::Receiver<()>,
    write_pool: SqlitePool,
) {
    loop {
        tokio::select! {
            biased; // Check shutdown first each iteration

            _ = &mut shutdown_rx => {
                // Shutdown signal received. Drain all remaining events and commit.
                drain_remaining_and_commit(&mut rx, &write_pool).await;
                return;
            }

            maybe_event = rx.recv() => {
                let first = match maybe_event {
                    Some(e) => e,
                    None => {
                        // Sender dropped (SqlxStore was dropped without close()).
                        // Drain whatever remains (none) and exit.
                        return;
                    }
                };

                // Collect batch: up to DRAIN_BATCH_SIZE events total.
                let mut batch = Vec::with_capacity(DRAIN_BATCH_SIZE);
                batch.push(first);

                // Non-blocking collection of additional events.
                while batch.len() < DRAIN_BATCH_SIZE {
                    match rx.try_recv() {
                        Ok(e) => batch.push(e),
                        Err(_) => break,  // Empty or closed; proceed with partial batch
                    }
                }

                // If batch is still under capacity, wait up to DRAIN_FLUSH_INTERVAL
                // for more events before committing the partial batch.
                if batch.len() < DRAIN_BATCH_SIZE {
                    let deadline = tokio::time::Instant::now() + DRAIN_FLUSH_INTERVAL;
                    loop {
                        match tokio::time::timeout_at(deadline, rx.recv()).await {
                            Ok(Some(e)) => {
                                batch.push(e);
                                if batch.len() >= DRAIN_BATCH_SIZE { break; }
                            }
                            Ok(None) | Err(_) => break, // Channel closed or timeout
                        }
                    }
                }

                commit_batch(batch, &write_pool).await;
            }
        }
    }
}
```

### `drain_remaining_and_commit`

```rust
/// Called when shutdown signal is received. Drains all pending events in the channel
/// and commits them. This is the final flush before task exit.
async fn drain_remaining_and_commit(
    rx: &mut mpsc::Receiver<AnalyticsWrite>,
    write_pool: &SqlitePool,
) {
    let mut remaining = Vec::new();
    while let Ok(e) = rx.try_recv() {
        remaining.push(e);
        // Commit in batches of DRAIN_BATCH_SIZE to avoid huge transactions.
        if remaining.len() >= DRAIN_BATCH_SIZE {
            commit_batch(std::mem::take(&mut remaining), write_pool).await;
        }
    }
    if !remaining.is_empty() {
        commit_batch(remaining, write_pool).await;
    }
}
```

### `commit_batch`

```rust
/// Commits a batch of AnalyticsWrite events in a single write_pool transaction.
/// On failure: logs at ERROR level and discards the batch (analytics loss is acceptable).
/// Does NOT retry (retrying risks double-writes).
async fn commit_batch(batch: Vec<AnalyticsWrite>, write_pool: &SqlitePool) {
    if batch.is_empty() { return; }

    let mut txn = match write_pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(
                batch_size = batch.len(),
                error = %e,
                "analytics drain: failed to acquire write connection; batch discarded"
            );
            return;
        }
    };

    for event in batch {
        if let Err(e) = execute_analytics_write(&mut txn, event).await {
            tracing::error!(error = %e, "analytics drain: write failed; rolling back batch");
            let _ = txn.rollback().await;
            return;
        }
    }

    if let Err(e) = txn.commit().await {
        tracing::error!(error = %e, "analytics drain: commit failed; batch discarded");
    }
}
```

### `execute_analytics_write`

```rust
/// Executes one AnalyticsWrite event within an open transaction.
/// Returns Err to trigger batch rollback on failure.
async fn execute_analytics_write(
    txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: AnalyticsWrite,
) -> Result<(), sqlx::Error> {
    match event {
        AnalyticsWrite::CoAccess { id_a, id_b } => {
            // Ensure id_a < id_b (schema CHECK constraint).
            let (a, b) = if id_a < id_b { (id_a, id_b) } else { (id_b, id_a) };
            let now = current_unix_seconds();
            sqlx::query!(
                "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated)
                 VALUES (?1, ?2, 1, ?3)
                 ON CONFLICT (entry_id_a, entry_id_b)
                 DO UPDATE SET count = count + 1, last_updated = excluded.last_updated",
                a as i64, b as i64, now as i64
            )
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::SessionUpdate {
            session_id, feature_cycle, agent_role, started_at,
            ended_at, status, compaction_count, outcome, total_injections, keywords,
        } => {
            sqlx::query!(
                "INSERT INTO sessions
                    (session_id, feature_cycle, agent_role, started_at, ended_at,
                     status, compaction_count, outcome, total_injections, keywords)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT (session_id) DO UPDATE SET
                    feature_cycle = excluded.feature_cycle,
                    agent_role = excluded.agent_role,
                    ended_at = excluded.ended_at,
                    status = excluded.status,
                    compaction_count = excluded.compaction_count,
                    outcome = excluded.outcome,
                    total_injections = excluded.total_injections,
                    keywords = excluded.keywords",
                session_id, feature_cycle, agent_role, started_at, ended_at,
                status, compaction_count, outcome, total_injections, keywords
            )
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::InjectionLog { session_id, entry_id, confidence, timestamp } => {
            sqlx::query!(
                "INSERT INTO injection_log (session_id, entry_id, confidence, timestamp)
                 VALUES (?1, ?2, ?3, ?4)",
                session_id, entry_id as i64, confidence, timestamp
            )
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::QueryLog {
            session_id, query_text, ts, result_count,
            result_entry_ids, similarity_scores, retrieval_mode, source,
        } => {
            sqlx::query!(
                "INSERT INTO query_log
                    (session_id, query_text, ts, result_count,
                     result_entry_ids, similarity_scores, retrieval_mode, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                session_id, query_text, ts, result_count,
                result_entry_ids, similarity_scores, retrieval_mode, source
            )
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::SignalQueue { session_id, created_at, entry_ids, signal_type, signal_source } => {
            sqlx::query!(
                "INSERT INTO signal_queue (session_id, created_at, entry_ids, signal_type, signal_source)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                session_id, created_at, entry_ids, signal_type, signal_source
            )
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::Observation {
            session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal,
        } => {
            sqlx::query!(
                "INSERT INTO observations
                    (session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal
            )
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::ObservationMetric { feature_cycle, computed_at, total_tool_calls,
            total_duration_secs, session_count, search_miss_rate, edit_bloat_total_kb,
            edit_bloat_ratio, permission_friction_events, bash_for_search_count,
            cold_restart_events, coordinator_respawn_count, parallel_call_rate,
            context_load_before_first_write_kb, total_context_loaded_kb,
            post_completion_work_pct, follow_up_issues_created, knowledge_entries_stored,
            sleep_workaround_count, agent_hotspot_count, friction_hotspot_count,
            session_hotspot_count, scope_hotspot_count,
        } => {
            sqlx::query!(
                "INSERT INTO observation_metrics
                    (feature_cycle, computed_at, total_tool_calls, total_duration_secs,
                     session_count, search_miss_rate, edit_bloat_total_kb, edit_bloat_ratio,
                     permission_friction_events, bash_for_search_count, cold_restart_events,
                     coordinator_respawn_count, parallel_call_rate,
                     context_load_before_first_write_kb, total_context_loaded_kb,
                     post_completion_work_pct, follow_up_issues_created,
                     knowledge_entries_stored, sleep_workaround_count,
                     agent_hotspot_count, friction_hotspot_count,
                     session_hotspot_count, scope_hotspot_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                         ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
                 ON CONFLICT (feature_cycle) DO UPDATE SET
                    computed_at = excluded.computed_at,
                    total_tool_calls = excluded.total_tool_calls,
                    /* ... all other columns ... */",
                feature_cycle, computed_at, total_tool_calls, total_duration_secs,
                session_count, search_miss_rate, edit_bloat_total_kb, edit_bloat_ratio,
                permission_friction_events, bash_for_search_count, cold_restart_events,
                coordinator_respawn_count, parallel_call_rate,
                context_load_before_first_write_kb, total_context_loaded_kb,
                post_completion_work_pct, follow_up_issues_created, knowledge_entries_stored,
                sleep_workaround_count, agent_hotspot_count, friction_hotspot_count,
                session_hotspot_count, scope_hotspot_count
            )
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::ShadowEvaluation {
            timestamp, rule_name, rule_category, neural_category,
            neural_confidence, convention_score, rule_accepted, digest,
        } => {
            sqlx::query!(
                "INSERT INTO shadow_evaluations
                    (timestamp, rule_name, rule_category, neural_category,
                     neural_confidence, convention_score, rule_accepted, digest)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                timestamp, rule_name, rule_category, neural_category,
                neural_confidence, convention_score, rule_accepted, digest
            )
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::FeatureEntry { feature_id, entry_id } => {
            sqlx::query!(
                "INSERT OR IGNORE INTO feature_entries (feature_id, entry_id) VALUES (?1, ?2)",
                feature_id, entry_id as i64
            )
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::TopicDelivery {
            topic, created_at, completed_at, status, github_issue,
            total_sessions, total_tool_calls, total_duration_secs, phases_completed,
        } => {
            sqlx::query!(
                "INSERT INTO topic_deliveries
                    (topic, created_at, completed_at, status, github_issue,
                     total_sessions, total_tool_calls, total_duration_secs, phases_completed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (topic) DO UPDATE SET
                    completed_at = excluded.completed_at,
                    status = excluded.status,
                    github_issue = excluded.github_issue,
                    total_sessions = excluded.total_sessions,
                    total_tool_calls = excluded.total_tool_calls,
                    total_duration_secs = excluded.total_duration_secs,
                    phases_completed = excluded.phases_completed",
                topic, created_at, completed_at, status, github_issue,
                total_sessions, total_tool_calls, total_duration_secs, phases_completed
            )
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::OutcomeIndex { feature_cycle, entry_id } => {
            sqlx::query!(
                "INSERT OR IGNORE INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)",
                feature_cycle, entry_id as i64
            )
            .execute(&mut **txn)
            .await?;
        }

        // Catch-all for future #[non_exhaustive] variants (FR-17).
        _ => {
            tracing::debug!("analytics drain: unknown AnalyticsWrite variant; skipping");
        }
    }
    Ok(())
}
```

Note: The `ObservationMetric` ON CONFLICT clause must list all non-primary-key columns
explicitly. The pseudocode shows the pattern; the implementation must expand the `/* ... */`
comment to enumerate all 22 non-primary-key columns.

---

## Helper

```rust
fn current_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
```

---

## Error Handling

- `commit_batch`: acquires write_pool connection; on failure logs ERROR and returns (no
  panic, no retry, no error propagation to callers — analytics loss is acceptable).
- `execute_analytics_write`: on any sqlx error, returns `Err` to caller (`commit_batch`)
  which rolls back the entire batch via `txn.rollback().await`.
- Drain task loop: on channel `None` (sender dropped), exits cleanly.
- `#[non_exhaustive]` catch-all in `execute_analytics_write`: logs DEBUG, returns `Ok(())`.

---

## Key Test Scenarios

1. **`test_analytics_write_shed_counter_increments`** (R-04): Fill channel to capacity
   (send 1000 events via `try_send`); send one more; assert `shed_events_total() == 1`;
   assert WARN log contains variant name, `queue_len=1000`, `capacity=1000`. (AC-06, AC-15)

2. **`test_analytics_write_shed_counter_cumulates`** (R-04): Induce N shed events;
   assert `shed_events_total() == N`.

3. **`test_drain_batch_size_exactly_50`** (edge case): Enqueue exactly 50 events;
   call `Store::close().await`; assert drain committed exactly 50 rows. (AC-06, NF-04)

4. **`test_drain_batch_size_51_two_batches`** (edge case): Enqueue 51 events;
   assert 51 rows committed across two batches (50 + 1).

5. **`test_drain_batch_size_1_waits_interval`** (edge case): Enqueue 1 event;
   assert drain waits approximately DRAIN_FLUSH_INTERVAL before committing; assert
   1 row committed after close.

6. **`test_drain_empty_shutdown`** (edge case): Close store with empty channel;
   assert `Store::close()` returns promptly (well under DRAIN_SHUTDOWN_TIMEOUT).

7. **`test_co_access_id_order_normalized`** (correctness): Send `CoAccess { id_a: 5, id_b: 3 }`;
   assert the row inserted has `entry_id_a=3`, `entry_id_b=5` (min/max normalization enforces
   the schema CHECK constraint).

8. **`test_unknown_variant_catch_all`** (FR-17): Extend enum with `#[cfg(test)]` test-only
   variant; assert drain task processes it via catch-all without panic.

9. **`test_observation_metric_all_23_fields`** (OQ-DURING-03): Write an `ObservationMetric`
   variant with all 23 fields; close store; read back via read_pool; assert all fields match.

10. **`test_integrity_write_survives_full_analytics_queue`** (R-06, AC-08): Fill queue to
    capacity; call a direct integrity write (write_entry); assert write succeeds and is
    readable without error.

---

## OQ-DURING Items Affecting This Component

- **OQ-DURING-03** (field completeness): All 11 variant field sets are reconciled above
  against schema v12 DDL. `ObservationMetric` has 23 fields (1 primary key + 22 non-PK).
  The sqlx::query!() macro will catch mismatches at compile time once sqlx-data.json is
  generated.
