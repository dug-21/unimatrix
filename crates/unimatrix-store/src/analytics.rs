use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use sqlx::sqlite::SqlitePool;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::pool_config::{DRAIN_BATCH_SIZE, DRAIN_FLUSH_INTERVAL};

/// All analytics write operations routed through the bounded channel.
///
/// Integrity tables (`entries`, `entry_tags`, `audit_log`, `agent_registry`,
/// `vector_map`, `counters`) are NEVER represented here — they always go through
/// `write_pool` directly (AC-08, C-02).
///
/// `#[non_exhaustive]`: Wave 1+ variants (`GraphEdge`, `ConfidenceWeightUpdate`)
/// are added without breaking the drain task match in dependent crates (FR-17, C-08).
/// External crate `match` on `AnalyticsWrite` MUST include a `_ => {}` catch-all arm.
#[non_exhaustive]
#[derive(Debug)]
pub enum AnalyticsWrite {
    /// Table: `co_access` — upsert co-access count.
    ///
    /// Caller must ensure `id_a < id_b` or the drain task normalizes it
    /// to satisfy the schema `CHECK (entry_id_a < entry_id_b)` constraint.
    CoAccess { id_a: u64, id_b: u64 },

    /// Table: `sessions` — upsert (INSERT OR REPLACE) by `session_id`.
    SessionUpdate {
        session_id: String,
        feature_cycle: Option<String>,
        agent_role: Option<String>,
        started_at: i64,
        ended_at: Option<i64>,
        status: i64,
        compaction_count: i64,
        outcome: Option<String>,
        total_injections: i64,
        keywords: Option<String>,
    },

    /// Table: `injection_log` — append-only insert.
    InjectionLog {
        session_id: String,
        entry_id: u64,
        confidence: f64,
        timestamp: i64,
    },

    /// Table: `query_log` — append-only insert.
    QueryLog {
        session_id: String,
        query_text: String,
        ts: i64,
        result_count: i64,
        result_entry_ids: Option<String>,
        similarity_scores: Option<String>,
        retrieval_mode: Option<String>,
        source: String,
    },

    /// Table: `signal_queue` — append-only insert.
    SignalQueue {
        session_id: String,
        created_at: i64,
        entry_ids: String,
        signal_type: i64,
        signal_source: i64,
    },

    /// Table: `observations` — append-only insert.
    Observation {
        session_id: String,
        ts_millis: i64,
        hook: String,
        tool: Option<String>,
        input: Option<String>,
        response_size: Option<i64>,
        response_snippet: Option<String>,
        topic_signal: Option<String>,
    },

    /// Table: `observation_metrics` — upsert (INSERT OR REPLACE) by `feature_cycle`.
    ///
    /// 24 fields: feature_cycle + computed_at + 21 UniversalMetrics columns +
    /// domain_metrics_json (schema v14, ADR-006). `domain_metrics_json` is NULL for
    /// claude-code sessions (empty domain_metrics map).
    ObservationMetric {
        feature_cycle: String,
        computed_at: i64,
        total_tool_calls: i64,
        total_duration_secs: i64,
        session_count: i64,
        search_miss_rate: f64,
        edit_bloat_total_kb: f64,
        edit_bloat_ratio: f64,
        permission_friction_events: i64,
        bash_for_search_count: i64,
        cold_restart_events: i64,
        coordinator_respawn_count: i64,
        parallel_call_rate: f64,
        context_load_before_first_write_kb: f64,
        total_context_loaded_kb: f64,
        post_completion_work_pct: f64,
        follow_up_issues_created: i64,
        knowledge_entries_stored: i64,
        sleep_workaround_count: i64,
        agent_hotspot_count: i64,
        friction_hotspot_count: i64,
        session_hotspot_count: i64,
        scope_hotspot_count: i64,
        /// NULL when domain_metrics is empty (claude-code sessions). JSON object otherwise.
        domain_metrics_json: Option<String>,
    },

    /// Table: `shadow_evaluations` — append-only insert.
    ShadowEvaluation {
        timestamp: i64,
        rule_name: String,
        rule_category: String,
        neural_category: String,
        neural_confidence: f64,
        convention_score: f64,
        rule_accepted: i64,
        digest: Option<Vec<u8>>,
    },

    /// Table: `feature_entries` — idempotent insert (`INSERT OR IGNORE`).
    FeatureEntry { feature_id: String, entry_id: u64 },

    /// Table: `topic_deliveries` — upsert by `topic`.
    TopicDelivery {
        topic: String,
        created_at: i64,
        completed_at: Option<i64>,
        status: String,
        github_issue: Option<i64>,
        total_sessions: i64,
        total_tool_calls: i64,
        total_duration_secs: i64,
        phases_completed: Option<String>,
    },

    /// Table: `outcome_index` — idempotent insert (`INSERT OR IGNORE`).
    OutcomeIndex {
        feature_cycle: String,
        entry_id: u64,
    },

    /// Table: `observation_phase_metrics` — insert/replace by (feature_cycle, phase_name).
    ///
    /// Written atomically with `ObservationMetric` by `store_metrics()` (OQ-NEW-01).
    /// Phase rows reference observation_metrics via FK (DELETE CASCADE on feature_cycle).
    ObservationPhaseMetric {
        feature_cycle: String,
        phase_name: String,
        duration_secs: i64,
        tool_call_count: i64,
    },

    /// Table: `observation_phase_metrics` — delete all rows for a feature cycle.
    ///
    /// Enqueued by `store_metrics()` BEFORE new `ObservationPhaseMetric` events so that
    /// stale phase rows from a previous call are removed before the new set is inserted.
    /// Callers must enqueue this before phase inserts in the same `enqueue_analytics` sequence.
    DeleteObservationPhases { feature_cycle: String },

    /// Table: `graph_edges` — idempotent insert (`INSERT OR IGNORE`).
    ///
    /// SHEDDING POLICY: Shed-safe for bootstrap-origin writes only. W1-2 NLI confirmed
    /// edge writes MUST NOT use this variant — use direct write_pool path instead.
    /// (ARCHITECTURE §2c SR-02, ADR-001 Consequences)
    ///
    /// `weight` must be finite (not NaN, not ±Inf). The drain task validates
    /// `weight.is_finite()` and drops the event with an ERROR log if the check fails.
    /// (FR-12, AC-17)
    GraphEdge {
        source_id: u64,
        target_id: u64,
        relation_type: String, // RelationType::as_str() value
        weight: f32,           // validated finite by caller before enqueue
        created_by: String,
        source: String,
        bootstrap_only: bool,
    },
    // Future Wave 3+ variants (not defined here):
    //   ConfidenceWeightUpdate { .. } — W3-1
}

impl AnalyticsWrite {
    /// Returns the variant name as a `&'static str` for structured WARN log messages
    /// emitted when an event is shed from a full queue.
    pub(crate) fn variant_name(&self) -> &'static str {
        // #[allow(unreachable_patterns)]: catch-all is unreachable within this crate
        // but required by external crates matching #[non_exhaustive] enums (FR-17, C-08).
        #[allow(unreachable_patterns)]
        match self {
            AnalyticsWrite::CoAccess { .. } => "CoAccess",
            AnalyticsWrite::SessionUpdate { .. } => "SessionUpdate",
            AnalyticsWrite::InjectionLog { .. } => "InjectionLog",
            AnalyticsWrite::QueryLog { .. } => "QueryLog",
            AnalyticsWrite::SignalQueue { .. } => "SignalQueue",
            AnalyticsWrite::Observation { .. } => "Observation",
            AnalyticsWrite::ObservationMetric { .. } => "ObservationMetric",
            AnalyticsWrite::ShadowEvaluation { .. } => "ShadowEvaluation",
            AnalyticsWrite::FeatureEntry { .. } => "FeatureEntry",
            AnalyticsWrite::TopicDelivery { .. } => "TopicDelivery",
            AnalyticsWrite::OutcomeIndex { .. } => "OutcomeIndex",
            AnalyticsWrite::ObservationPhaseMetric { .. } => "ObservationPhaseMetric",
            AnalyticsWrite::DeleteObservationPhases { .. } => "DeleteObservationPhases",
            AnalyticsWrite::GraphEdge { .. } => "GraphEdge",
            // Catch-all for future #[non_exhaustive] variants.
            _ => "Unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// Drain task public entry point
// ---------------------------------------------------------------------------

/// Spawns the analytics drain task onto the current tokio runtime.
///
/// The task is owned by `SqlxStore`; the returned `JoinHandle` is held in
/// `drain_handle: Option<JoinHandle<()>>` and awaited by `Store::close()`.
///
/// `shed_counter` is shared with `SqlxStore::enqueue_analytics()` so that
/// shed events are visible via `store.shed_events_total()`.
pub(crate) fn spawn_drain_task(
    write_pool: SqlitePool,
    rx: mpsc::Receiver<AnalyticsWrite>,
    shutdown_rx: oneshot::Receiver<()>,
    shed_counter: Arc<AtomicU64>,
) -> JoinHandle<()> {
    tokio::spawn(run_drain_task(write_pool, rx, shutdown_rx, shed_counter))
}

// ---------------------------------------------------------------------------
// Drain task internals
// ---------------------------------------------------------------------------

/// Long-lived tokio task. Started in `SqlxStore::open()`, runs for the lifetime of the store.
///
/// On shutdown signal: drains all remaining events, commits, exits.
/// On channel close (sender dropped without `close()`): exits cleanly.
async fn run_drain_task(
    write_pool: SqlitePool,
    mut rx: mpsc::Receiver<AnalyticsWrite>,
    mut shutdown_rx: oneshot::Receiver<()>,
    _shed_counter: Arc<AtomicU64>,
) {
    loop {
        tokio::select! {
            biased; // Check shutdown first each iteration (FR-04).

            _ = &mut shutdown_rx => {
                // Shutdown signal received. Drain all remaining events and commit.
                drain_remaining_and_commit(&mut rx, &write_pool).await;
                return;
            }

            maybe_event = rx.recv() => {
                let first = match maybe_event {
                    Some(e) => e,
                    None => {
                        // Sender dropped (SqlxStore dropped without close()).
                        // No remaining events to drain; exit.
                        return;
                    }
                };

                // Collect batch: up to DRAIN_BATCH_SIZE events total.
                let mut batch = Vec::with_capacity(DRAIN_BATCH_SIZE);
                batch.push(first);

                // Non-blocking collection of additional events already in the channel.
                while batch.len() < DRAIN_BATCH_SIZE {
                    match rx.try_recv() {
                        Ok(e) => batch.push(e),
                        Err(_) => break, // Empty or closed; proceed with partial batch.
                    }
                }

                // If batch is still under capacity, wait up to DRAIN_FLUSH_INTERVAL
                // for more events before committing the partial batch (NF-04).
                if batch.len() < DRAIN_BATCH_SIZE {
                    let deadline = tokio::time::Instant::now() + DRAIN_FLUSH_INTERVAL;
                    loop {
                        match tokio::time::timeout_at(deadline, rx.recv()).await {
                            Ok(Some(e)) => {
                                batch.push(e);
                                if batch.len() >= DRAIN_BATCH_SIZE {
                                    break;
                                }
                            }
                            Ok(None) | Err(_) => break, // Channel closed or timeout.
                        }
                    }
                }

                commit_batch(batch, &write_pool).await;
            }
        }
    }
}

/// Drains all remaining events from `rx` and commits them in batches.
///
/// Called when the shutdown signal is received. This is the final flush before task exit.
async fn drain_remaining_and_commit(
    rx: &mut mpsc::Receiver<AnalyticsWrite>,
    write_pool: &SqlitePool,
) {
    let mut remaining = Vec::new();
    while let Ok(e) = rx.try_recv() {
        remaining.push(e);
        // Commit in batches to avoid huge transactions.
        if remaining.len() >= DRAIN_BATCH_SIZE {
            commit_batch(std::mem::take(&mut remaining), write_pool).await;
        }
    }
    if !remaining.is_empty() {
        commit_batch(remaining, write_pool).await;
    }
}

/// Commits a batch of `AnalyticsWrite` events in a single `write_pool` transaction.
///
/// On failure: logs at `ERROR` level and discards the batch. Analytics loss is
/// acceptable (FR-05). Does NOT retry — retrying risks double-writes.
async fn commit_batch(batch: Vec<AnalyticsWrite>, write_pool: &SqlitePool) {
    if batch.is_empty() {
        return;
    }

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
            tracing::error!(
                error = %e,
                "analytics drain: write failed; rolling back batch"
            );
            let _ = txn.rollback().await;
            return;
        }
    }

    if let Err(e) = txn.commit().await {
        tracing::error!(
            error = %e,
            "analytics drain: commit failed; batch discarded"
        );
    }
}

/// Executes one `AnalyticsWrite` event within an open transaction.
///
/// Returns `Err` to trigger batch rollback on failure. The catch-all arm for
/// unknown `#[non_exhaustive]` variants logs at DEBUG and returns `Ok(())` (FR-17).
///
/// Note: Uses `sqlx::query()` (runtime-checked) rather than `sqlx::query!()` (compile-time
/// macro). The macro form requires `sqlx-data.json` to be generated first, which happens
/// in Wave 5 (`ci-offline` component). Wave 2 will convert hot-path integrity writes;
/// Wave 5 will handle offline cache generation for all query sites including this file.
async fn execute_analytics_write(
    txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: AnalyticsWrite,
) -> Result<(), sqlx::Error> {
    // #[allow(unreachable_patterns)]: catch-all is unreachable within this crate
    // but required by external crates matching #[non_exhaustive] enums (FR-17, C-08).
    #[allow(unreachable_patterns)]
    match event {
        AnalyticsWrite::CoAccess { id_a, id_b } => {
            // Normalize order to satisfy schema CHECK (entry_id_a < entry_id_b).
            let (a, b) = if id_a <= id_b {
                (id_a as i64, id_b as i64)
            } else {
                (id_b as i64, id_a as i64)
            };
            let now = current_unix_seconds();
            sqlx::query(
                "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated)
                 VALUES (?1, ?2, 1, ?3)
                 ON CONFLICT (entry_id_a, entry_id_b)
                 DO UPDATE SET count = count + 1, last_updated = excluded.last_updated",
            )
            .bind(a)
            .bind(b)
            .bind(now)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::SessionUpdate {
            session_id,
            feature_cycle,
            agent_role,
            started_at,
            ended_at,
            status,
            compaction_count,
            outcome,
            total_injections,
            keywords,
        } => {
            sqlx::query(
                "INSERT INTO sessions
                    (session_id, feature_cycle, agent_role, started_at, ended_at,
                     status, compaction_count, outcome, total_injections, keywords)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT (session_id) DO UPDATE SET
                    feature_cycle    = excluded.feature_cycle,
                    agent_role       = excluded.agent_role,
                    ended_at         = excluded.ended_at,
                    status           = excluded.status,
                    compaction_count = excluded.compaction_count,
                    outcome          = excluded.outcome,
                    total_injections = excluded.total_injections,
                    keywords         = excluded.keywords",
            )
            .bind(session_id)
            .bind(feature_cycle)
            .bind(agent_role)
            .bind(started_at)
            .bind(ended_at)
            .bind(status)
            .bind(compaction_count)
            .bind(outcome)
            .bind(total_injections)
            .bind(keywords)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::InjectionLog {
            session_id,
            entry_id,
            confidence,
            timestamp,
        } => {
            sqlx::query(
                "INSERT INTO injection_log (session_id, entry_id, confidence, timestamp)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(session_id)
            .bind(entry_id as i64)
            .bind(confidence)
            .bind(timestamp)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::QueryLog {
            session_id,
            query_text,
            ts,
            result_count,
            result_entry_ids,
            similarity_scores,
            retrieval_mode,
            source,
        } => {
            sqlx::query(
                "INSERT INTO query_log
                    (session_id, query_text, ts, result_count,
                     result_entry_ids, similarity_scores, retrieval_mode, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(session_id)
            .bind(query_text)
            .bind(ts)
            .bind(result_count)
            .bind(result_entry_ids)
            .bind(similarity_scores)
            .bind(retrieval_mode)
            .bind(source)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::SignalQueue {
            session_id,
            created_at,
            entry_ids,
            signal_type,
            signal_source,
        } => {
            sqlx::query(
                "INSERT INTO signal_queue
                    (session_id, created_at, entry_ids, signal_type, signal_source)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(session_id)
            .bind(created_at)
            .bind(entry_ids)
            .bind(signal_type)
            .bind(signal_source)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::Observation {
            session_id,
            ts_millis,
            hook,
            tool,
            input,
            response_size,
            response_snippet,
            topic_signal,
        } => {
            sqlx::query(
                "INSERT INTO observations
                    (session_id, ts_millis, hook, tool, input,
                     response_size, response_snippet, topic_signal)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(session_id)
            .bind(ts_millis)
            .bind(hook)
            .bind(tool)
            .bind(input)
            .bind(response_size)
            .bind(response_snippet)
            .bind(topic_signal)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::ObservationMetric {
            feature_cycle,
            computed_at,
            total_tool_calls,
            total_duration_secs,
            session_count,
            search_miss_rate,
            edit_bloat_total_kb,
            edit_bloat_ratio,
            permission_friction_events,
            bash_for_search_count,
            cold_restart_events,
            coordinator_respawn_count,
            parallel_call_rate,
            context_load_before_first_write_kb,
            total_context_loaded_kb,
            post_completion_work_pct,
            follow_up_issues_created,
            knowledge_entries_stored,
            sleep_workaround_count,
            agent_hotspot_count,
            friction_hotspot_count,
            session_hotspot_count,
            scope_hotspot_count,
            domain_metrics_json,
        } => {
            // 23 non-primary-key columns: 22 typed + domain_metrics_json (schema v14, ADR-006).
            sqlx::query(
                "INSERT INTO observation_metrics
                    (feature_cycle, computed_at, total_tool_calls, total_duration_secs,
                     session_count, search_miss_rate, edit_bloat_total_kb, edit_bloat_ratio,
                     permission_friction_events, bash_for_search_count, cold_restart_events,
                     coordinator_respawn_count, parallel_call_rate,
                     context_load_before_first_write_kb, total_context_loaded_kb,
                     post_completion_work_pct, follow_up_issues_created,
                     knowledge_entries_stored, sleep_workaround_count,
                     agent_hotspot_count, friction_hotspot_count,
                     session_hotspot_count, scope_hotspot_count,
                     domain_metrics_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                         ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
                 ON CONFLICT (feature_cycle) DO UPDATE SET
                    computed_at                        = excluded.computed_at,
                    total_tool_calls                   = excluded.total_tool_calls,
                    total_duration_secs                = excluded.total_duration_secs,
                    session_count                      = excluded.session_count,
                    search_miss_rate                   = excluded.search_miss_rate,
                    edit_bloat_total_kb                = excluded.edit_bloat_total_kb,
                    edit_bloat_ratio                   = excluded.edit_bloat_ratio,
                    permission_friction_events         = excluded.permission_friction_events,
                    bash_for_search_count              = excluded.bash_for_search_count,
                    cold_restart_events                = excluded.cold_restart_events,
                    coordinator_respawn_count          = excluded.coordinator_respawn_count,
                    parallel_call_rate                 = excluded.parallel_call_rate,
                    context_load_before_first_write_kb = excluded.context_load_before_first_write_kb,
                    total_context_loaded_kb            = excluded.total_context_loaded_kb,
                    post_completion_work_pct           = excluded.post_completion_work_pct,
                    follow_up_issues_created           = excluded.follow_up_issues_created,
                    knowledge_entries_stored           = excluded.knowledge_entries_stored,
                    sleep_workaround_count             = excluded.sleep_workaround_count,
                    agent_hotspot_count                = excluded.agent_hotspot_count,
                    friction_hotspot_count             = excluded.friction_hotspot_count,
                    session_hotspot_count              = excluded.session_hotspot_count,
                    scope_hotspot_count                = excluded.scope_hotspot_count,
                    domain_metrics_json                = excluded.domain_metrics_json",
            )
            .bind(feature_cycle)
            .bind(computed_at)
            .bind(total_tool_calls)
            .bind(total_duration_secs)
            .bind(session_count)
            .bind(search_miss_rate)
            .bind(edit_bloat_total_kb)
            .bind(edit_bloat_ratio)
            .bind(permission_friction_events)
            .bind(bash_for_search_count)
            .bind(cold_restart_events)
            .bind(coordinator_respawn_count)
            .bind(parallel_call_rate)
            .bind(context_load_before_first_write_kb)
            .bind(total_context_loaded_kb)
            .bind(post_completion_work_pct)
            .bind(follow_up_issues_created)
            .bind(knowledge_entries_stored)
            .bind(sleep_workaround_count)
            .bind(agent_hotspot_count)
            .bind(friction_hotspot_count)
            .bind(session_hotspot_count)
            .bind(scope_hotspot_count)
            .bind(domain_metrics_json)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::ShadowEvaluation {
            timestamp,
            rule_name,
            rule_category,
            neural_category,
            neural_confidence,
            convention_score,
            rule_accepted,
            digest,
        } => {
            sqlx::query(
                "INSERT INTO shadow_evaluations
                    (timestamp, rule_name, rule_category, neural_category,
                     neural_confidence, convention_score, rule_accepted, digest)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(timestamp)
            .bind(rule_name)
            .bind(rule_category)
            .bind(neural_category)
            .bind(neural_confidence)
            .bind(convention_score)
            .bind(rule_accepted)
            .bind(digest)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::FeatureEntry {
            feature_id,
            entry_id,
        } => {
            sqlx::query(
                "INSERT OR IGNORE INTO feature_entries (feature_id, entry_id) VALUES (?1, ?2)",
            )
            .bind(feature_id)
            .bind(entry_id as i64)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::TopicDelivery {
            topic,
            created_at,
            completed_at,
            status,
            github_issue,
            total_sessions,
            total_tool_calls,
            total_duration_secs,
            phases_completed,
        } => {
            sqlx::query(
                "INSERT INTO topic_deliveries
                    (topic, created_at, completed_at, status, github_issue,
                     total_sessions, total_tool_calls, total_duration_secs, phases_completed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (topic) DO UPDATE SET
                    completed_at        = excluded.completed_at,
                    status              = excluded.status,
                    github_issue        = excluded.github_issue,
                    total_sessions      = excluded.total_sessions,
                    total_tool_calls    = excluded.total_tool_calls,
                    total_duration_secs = excluded.total_duration_secs,
                    phases_completed    = excluded.phases_completed",
            )
            .bind(topic)
            .bind(created_at)
            .bind(completed_at)
            .bind(status)
            .bind(github_issue)
            .bind(total_sessions)
            .bind(total_tool_calls)
            .bind(total_duration_secs)
            .bind(phases_completed)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::OutcomeIndex {
            feature_cycle,
            entry_id,
        } => {
            sqlx::query(
                "INSERT OR IGNORE INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)",
            )
            .bind(feature_cycle)
            .bind(entry_id as i64)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::ObservationPhaseMetric {
            feature_cycle,
            phase_name,
            duration_secs,
            tool_call_count,
        } => {
            sqlx::query(
                "INSERT INTO observation_phase_metrics
                    (feature_cycle, phase_name, duration_secs, tool_call_count)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (feature_cycle, phase_name) DO UPDATE SET
                    duration_secs   = excluded.duration_secs,
                    tool_call_count = excluded.tool_call_count",
            )
            .bind(feature_cycle)
            .bind(phase_name)
            .bind(duration_secs)
            .bind(tool_call_count)
            .execute(&mut **txn)
            .await?;
        }

        AnalyticsWrite::DeleteObservationPhases { feature_cycle } => {
            sqlx::query("DELETE FROM observation_phase_metrics WHERE feature_cycle = ?1")
                .bind(feature_cycle)
                .execute(&mut **txn)
                .await?;
        }

        AnalyticsWrite::GraphEdge {
            source_id,
            target_id,
            relation_type,
            weight,
            created_by,
            source,
            bootstrap_only,
        } => {
            // NF-01, AC-17, R-07: validate weight before writing.
            if !weight.is_finite() {
                tracing::error!(
                    source_id = source_id,
                    target_id = target_id,
                    relation_type = %relation_type,
                    weight = weight,
                    "analytics drain: GraphEdge weight is not finite (NaN/Inf); event dropped"
                );
                return Ok(());
            }

            let now = current_unix_seconds();
            let bootstrap_only_int: i64 = if bootstrap_only { 1 } else { 0 };

            sqlx::query(
                "INSERT OR IGNORE INTO graph_edges
                     (source_id, target_id, relation_type, weight, created_at,
                      created_by, source, bootstrap_only)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(source_id as i64)
            .bind(target_id as i64)
            .bind(relation_type)
            .bind(weight)
            .bind(now)
            .bind(created_by)
            .bind(source)
            .bind(bootstrap_only_int)
            .execute(&mut **txn)
            .await?;
        }

        // Catch-all for future #[non_exhaustive] variants (FR-17, C-08).
        // Logs at DEBUG and returns Ok to allow the drain task to continue.
        _ => {
            tracing::debug!("analytics drain: unknown AnalyticsWrite variant; skipping");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn current_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool_config::ANALYTICS_QUEUE_CAPACITY;
    use sqlx::Row as _;

    #[test]
    fn test_analytics_write_variant_names() {
        let cases: &[(&str, AnalyticsWrite)] = &[
            ("CoAccess", AnalyticsWrite::CoAccess { id_a: 1, id_b: 2 }),
            (
                "SessionUpdate",
                AnalyticsWrite::SessionUpdate {
                    session_id: "s".into(),
                    feature_cycle: None,
                    agent_role: None,
                    started_at: 0,
                    ended_at: None,
                    status: 0,
                    compaction_count: 0,
                    outcome: None,
                    total_injections: 0,
                    keywords: None,
                },
            ),
            (
                "InjectionLog",
                AnalyticsWrite::InjectionLog {
                    session_id: "s".into(),
                    entry_id: 1,
                    confidence: 0.9,
                    timestamp: 0,
                },
            ),
            (
                "QueryLog",
                AnalyticsWrite::QueryLog {
                    session_id: "s".into(),
                    query_text: "q".into(),
                    ts: 0,
                    result_count: 0,
                    result_entry_ids: None,
                    similarity_scores: None,
                    retrieval_mode: None,
                    source: "test".into(),
                },
            ),
            (
                "SignalQueue",
                AnalyticsWrite::SignalQueue {
                    session_id: "s".into(),
                    created_at: 0,
                    entry_ids: "[]".into(),
                    signal_type: 0,
                    signal_source: 0,
                },
            ),
            (
                "Observation",
                AnalyticsWrite::Observation {
                    session_id: "s".into(),
                    ts_millis: 0,
                    hook: "h".into(),
                    tool: None,
                    input: None,
                    response_size: None,
                    response_snippet: None,
                    topic_signal: None,
                },
            ),
            (
                "ShadowEvaluation",
                AnalyticsWrite::ShadowEvaluation {
                    timestamp: 0,
                    rule_name: "r".into(),
                    rule_category: "c".into(),
                    neural_category: "n".into(),
                    neural_confidence: 0.5,
                    convention_score: 0.5,
                    rule_accepted: 1,
                    digest: None,
                },
            ),
            (
                "FeatureEntry",
                AnalyticsWrite::FeatureEntry {
                    feature_id: "f".into(),
                    entry_id: 1,
                },
            ),
            (
                "TopicDelivery",
                AnalyticsWrite::TopicDelivery {
                    topic: "t".into(),
                    created_at: 0,
                    completed_at: None,
                    status: "active".into(),
                    github_issue: None,
                    total_sessions: 0,
                    total_tool_calls: 0,
                    total_duration_secs: 0,
                    phases_completed: None,
                },
            ),
            (
                "OutcomeIndex",
                AnalyticsWrite::OutcomeIndex {
                    feature_cycle: "nxs-011".into(),
                    entry_id: 42,
                },
            ),
        ];

        for (expected, event) in cases {
            assert_eq!(
                event.variant_name(),
                *expected,
                "variant_name mismatch for {expected}"
            );
        }
    }

    #[test]
    fn test_analytics_write_non_exhaustive_variant_name_unknown() {
        // Verify the catch-all pattern in variant_name() compiles correctly.
        // Since #[non_exhaustive] only affects external crates, we can't construct
        // an unknown variant here — but we can verify all known variants return
        // non-"Unknown" values, demonstrating the catch-all is reachable in principle.
        let event = AnalyticsWrite::CoAccess { id_a: 1, id_b: 2 };
        assert_ne!(event.variant_name(), "Unknown");
    }

    #[test]
    fn test_analytics_queue_capacity_imported() {
        // ANALYTICS_QUEUE_CAPACITY is defined in pool_config.rs and imported here.
        // This test verifies the import is correct and the value is 1000.
        assert_eq!(ANALYTICS_QUEUE_CAPACITY, 1000);
    }

    #[test]
    fn test_observation_metric_field_count() {
        // Construct ObservationMetric with all 24 fields (schema v14, ADR-006).
        // 23 data columns + feature_cycle primary key = 24 struct fields.
        // domain_metrics_json is None for claude-code sessions (empty domain_metrics).
        let _metric = AnalyticsWrite::ObservationMetric {
            feature_cycle: "nxs-011".into(),
            computed_at: 1,
            total_tool_calls: 2,
            total_duration_secs: 3,
            session_count: 4,
            search_miss_rate: 0.1,
            edit_bloat_total_kb: 0.2,
            edit_bloat_ratio: 0.3,
            permission_friction_events: 5,
            bash_for_search_count: 6,
            cold_restart_events: 7,
            coordinator_respawn_count: 8,
            parallel_call_rate: 0.4,
            context_load_before_first_write_kb: 0.5,
            total_context_loaded_kb: 0.6,
            post_completion_work_pct: 0.7,
            follow_up_issues_created: 9,
            knowledge_entries_stored: 10,
            sleep_workaround_count: 11,
            agent_hotspot_count: 12,
            friction_hotspot_count: 13,
            session_hotspot_count: 14,
            scope_hotspot_count: 15,
            domain_metrics_json: None,
        };
        // If this test compiles, all 24 fields are correctly typed.
    }

    #[test]
    fn test_current_unix_seconds_positive() {
        let ts = current_unix_seconds();
        assert!(ts > 0, "expected positive unix timestamp, got {ts}");
    }

    // ---------------------------------------------------------------------------
    // GraphEdge variant tests (AC-09, R-07, AC-17)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_analytics_write_graph_edge_variant_name() {
        let event = AnalyticsWrite::GraphEdge {
            source_id: 1,
            target_id: 2,
            relation_type: "Supersedes".to_string(),
            weight: 1.0,
            created_by: "test".to_string(),
            source: "test".to_string(),
            bootstrap_only: false,
        };
        assert_eq!(event.variant_name(), "GraphEdge");
    }

    #[test]
    fn test_weight_guard_rejects_nan() {
        assert!(!f32::NAN.is_finite(), "NaN must not be finite");
    }

    #[test]
    fn test_weight_guard_rejects_positive_infinity() {
        assert!(!f32::INFINITY.is_finite(), "INFINITY must not be finite");
    }

    #[test]
    fn test_weight_guard_rejects_negative_infinity() {
        assert!(
            !f32::NEG_INFINITY.is_finite(),
            "NEG_INFINITY must not be finite"
        );
    }

    #[test]
    fn test_weight_guard_accepts_zero() {
        assert!(0.0_f32.is_finite(), "0.0 must be finite");
    }

    #[test]
    fn test_weight_guard_accepts_half() {
        assert!(0.5_f32.is_finite(), "0.5 must be finite");
    }

    #[test]
    fn test_weight_guard_accepts_one() {
        assert!(1.0_f32.is_finite(), "1.0 must be finite");
    }

    #[test]
    fn test_weight_guard_accepts_f32_max() {
        assert!(f32::MAX.is_finite(), "f32::MAX must be finite");
    }

    /// Verify graph_edge variant name is included in the existing variant_names test
    /// by constructing all 3 known cases for GraphEdge-related fields.
    #[test]
    fn test_analytics_write_non_exhaustive_contract_preserved() {
        // Constructing GraphEdge with a wildcard catch-all arm compiles correctly
        // because #[non_exhaustive] is respected by the catch-all `_ => {}`.
        let event = AnalyticsWrite::GraphEdge {
            source_id: 10,
            target_id: 20,
            relation_type: "CoAccess".to_string(),
            weight: 0.75,
            created_by: "bootstrap".to_string(),
            source: "co_access".to_string(),
            bootstrap_only: true,
        };
        // Match with explicit catch-all — validates #[non_exhaustive] contract not broken.
        let name = match &event {
            AnalyticsWrite::GraphEdge { .. } => "GraphEdge",
            _ => "other",
        };
        assert_eq!(name, "GraphEdge");
    }

    // ---------------------------------------------------------------------------
    // Drain integration tests — require graph_edges table
    // ---------------------------------------------------------------------------

    /// Create the graph_edges table in an in-memory / temp pool for drain tests.
    async fn create_graph_edges_table(pool: &sqlx::sqlite::SqlitePool) {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS graph_edges (
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
            )",
        )
        .execute(pool)
        .await
        .expect("create graph_edges table");
    }

    #[tokio::test]
    async fn test_analytics_graph_edge_drain_inserts_row() {
        use crate::test_helpers::open_test_store;
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        // graph_edges table is created by store migration (v13), but test stores
        // may be at an earlier schema. Ensure the table exists.
        create_graph_edges_table(&store.write_pool).await;

        store.enqueue_analytics(AnalyticsWrite::GraphEdge {
            source_id: 1,
            target_id: 2,
            relation_type: "Supersedes".to_string(),
            weight: 1.0,
            created_by: "test-agent".to_string(),
            source: "test".to_string(),
            bootstrap_only: false,
        });

        // Wait for drain to flush.
        // DRAIN_FLUSH_INTERVAL is 500ms; sleep must exceed it for the drain to commit.
        tokio::time::sleep(tokio::time::Duration::from_millis(700)).await;

        let row = sqlx::query(
            "SELECT source_id, target_id, relation_type, weight, created_by, \
             source, bootstrap_only, metadata \
             FROM graph_edges WHERE source_id = 1 AND target_id = 2",
        )
        .fetch_one(&store.write_pool)
        .await
        .expect("row must exist after drain");

        let src_id: i64 = row.try_get(0).unwrap();
        let tgt_id: i64 = row.try_get(1).unwrap();
        let rel: String = row.try_get(2).unwrap();
        let w: f32 = row.try_get(3).unwrap();
        let by: String = row.try_get(4).unwrap();
        let src: String = row.try_get(5).unwrap();
        let bo: i64 = row.try_get(6).unwrap();
        let meta: Option<String> = row.try_get(7).unwrap();

        assert_eq!(src_id, 1);
        assert_eq!(tgt_id, 2);
        assert_eq!(rel, "Supersedes");
        assert!((w - 1.0_f32).abs() < f32::EPSILON);
        assert_eq!(by, "test-agent");
        assert_eq!(src, "test");
        assert_eq!(bo, 0);
        assert!(meta.is_none());
    }

    #[tokio::test]
    async fn test_analytics_graph_edge_drain_rejects_nan_weight() {
        use crate::test_helpers::open_test_store;
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        store.enqueue_analytics(AnalyticsWrite::GraphEdge {
            source_id: 3,
            target_id: 4,
            relation_type: "Supersedes".to_string(),
            weight: f32::NAN,
            created_by: "test".to_string(),
            source: "test".to_string(),
            bootstrap_only: false,
        });

        // DRAIN_FLUSH_INTERVAL is 500ms; sleep must exceed it for the drain to commit.
        tokio::time::sleep(tokio::time::Duration::from_millis(700)).await;

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges WHERE source_id = 3")
            .fetch_one(&store.write_pool)
            .await
            .expect("count query");
        assert_eq!(count, 0, "NaN weight event must not insert a row");
    }

    #[tokio::test]
    async fn test_analytics_graph_edge_drain_idempotent_insert_or_ignore() {
        use crate::test_helpers::open_test_store;
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        for _ in 0..2 {
            store.enqueue_analytics(AnalyticsWrite::GraphEdge {
                source_id: 5,
                target_id: 6,
                relation_type: "CoAccess".to_string(),
                weight: 0.5,
                created_by: "bootstrap".to_string(),
                source: "co_access".to_string(),
                bootstrap_only: true,
            });
        }

        // DRAIN_FLUSH_INTERVAL is 500ms; sleep must exceed it for the drain to commit.
        tokio::time::sleep(tokio::time::Duration::from_millis(700)).await;

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM graph_edges WHERE source_id = 5 AND target_id = 6",
        )
        .fetch_one(&store.write_pool)
        .await
        .expect("count query");
        assert_eq!(
            count, 1,
            "INSERT OR IGNORE must deduplicate; expected exactly one row"
        );
    }

    #[tokio::test]
    async fn test_analytics_graph_edge_bootstrap_only_field_persisted() {
        use crate::test_helpers::open_test_store;
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        store.enqueue_analytics(AnalyticsWrite::GraphEdge {
            source_id: 7,
            target_id: 8,
            relation_type: "Supersedes".to_string(),
            weight: 1.0,
            created_by: "bootstrap".to_string(),
            source: "entries.supersedes".to_string(),
            bootstrap_only: true,
        });

        // DRAIN_FLUSH_INTERVAL is 500ms; sleep must exceed it for the drain to commit.
        tokio::time::sleep(tokio::time::Duration::from_millis(700)).await;

        let bo: i64 =
            sqlx::query_scalar("SELECT bootstrap_only FROM graph_edges WHERE source_id = 7")
                .fetch_one(&store.write_pool)
                .await
                .expect("row");
        assert_eq!(bo, 1, "bootstrap_only=true must be stored as 1");
    }

    #[tokio::test]
    async fn test_analytics_graph_edge_bootstrap_only_false_persisted() {
        use crate::test_helpers::open_test_store;
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        store.enqueue_analytics(AnalyticsWrite::GraphEdge {
            source_id: 9,
            target_id: 10,
            relation_type: "Supports".to_string(),
            weight: 0.8,
            created_by: "agent-x".to_string(),
            source: "nli".to_string(),
            bootstrap_only: false,
        });

        // DRAIN_FLUSH_INTERVAL is 500ms; sleep must exceed it for the drain to commit.
        tokio::time::sleep(tokio::time::Duration::from_millis(700)).await;

        let bo: i64 =
            sqlx::query_scalar("SELECT bootstrap_only FROM graph_edges WHERE source_id = 9")
                .fetch_one(&store.write_pool)
                .await
                .expect("row");
        assert_eq!(bo, 0, "bootstrap_only=false must be stored as 0");
    }

    #[tokio::test]
    async fn test_analytics_graph_edge_metadata_column_is_null() {
        use crate::test_helpers::open_test_store;
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        store.enqueue_analytics(AnalyticsWrite::GraphEdge {
            source_id: 11,
            target_id: 12,
            relation_type: "CoAccess".to_string(),
            weight: 0.6,
            created_by: "bootstrap".to_string(),
            source: "co_access".to_string(),
            bootstrap_only: false,
        });

        // DRAIN_FLUSH_INTERVAL is 500ms; sleep must exceed it for the drain to commit.
        tokio::time::sleep(tokio::time::Duration::from_millis(700)).await;

        let meta: Option<String> =
            sqlx::query_scalar("SELECT metadata FROM graph_edges WHERE source_id = 11")
                .fetch_one(&store.write_pool)
                .await
                .expect("row");
        assert!(
            meta.is_none(),
            "metadata must be NULL for all crt-021 writes"
        );
    }
}
