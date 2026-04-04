//! Behavioral signal logic for crt-046.
//!
//! Owns all behavioral signal computation extracted from `mcp/tools.rs`
//! to keep that file under the 500-line cap (ADR-001 crt-046).
//!
//! Provides six `pub(crate)` functions:
//! - `collect_coaccess_entry_ids` — parse entry IDs from `ObservationRow` slices
//! - `build_coaccess_pairs` — enumerate and cap co-access pairs per session
//! - `outcome_to_weight` — map cycle outcome string to edge weight
//! - `emit_behavioral_edges` — write bidirectional `Informs` edges via `write_pool_server()`
//! - `populate_goal_cluster` — write one `goal_clusters` row
//! - `blend_cluster_entries` — pure score-based interleaving for briefing blending
//!
//! ## write_graph_edge Return Contract (pattern #4041)
//!
//! ALL counter increments in `emit_behavioral_edges` are governed by this contract.
//! Any deviation is a bug — root cause of the crt-040 Gate 3a rework.
//!
//! | `write_graph_edge` return | Meaning                            | Counter action                   |
//! |---------------------------|------------------------------------|----------------------------------|
//! | `Ok(true)`                | New row inserted (rows_affected==1) | Increment `edges_enqueued`      |
//! | `Ok(false)`               | UNIQUE conflict — INSERT OR IGNORE  | Do NOT increment; not an error  |
//! | `Err(_)`                  | SQL infrastructure failure          | Log `warn!`, do NOT increment   |

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use tracing::{debug, warn};
use unimatrix_store::SqlxStore;
use unimatrix_store::{ObservationRow, Result, StoreError};

use crate::mcp::response::IndexEntry;

// ---------------------------------------------------------------------------
// Module-level constants
// ---------------------------------------------------------------------------

/// Maximum number of candidate rows scanned from `goal_clusters` at briefing time.
///
/// O(RECENCY_CAP × D) at D=384 is ~0.1ms — well within latency budget.
/// If this cap must be raised above ~10,000, move cosine computation to `spawn_blocking`.
pub(crate) const RECENCY_CAP: u64 = 100;

/// Maximum canonical co-access pairs extracted per cycle.
///
/// Enforced at enumeration time (halt when `pairs.len() == PAIR_CAP`),
/// not by post-hoc truncation. NFR-04.
pub(crate) const PAIR_CAP: usize = 200;

// ---------------------------------------------------------------------------
// Module-private helper: write_graph_edge
// ---------------------------------------------------------------------------

/// Write a single directed graph edge via `INSERT OR IGNORE INTO graph_edges`.
///
/// Returns `Ok(true)` when a new row was inserted (rows_affected == 1).
/// Returns `Ok(false)` on UNIQUE conflict — INSERT OR IGNORE silent no-op.
/// Returns `Err(_)` on SQL infrastructure failure.
///
/// Uses `store.write_pool_server()` directly — NOT the analytics drain (ADR-006 crt-046).
/// `source = 'behavioral'`, `relation_type = 'Informs'`, `bootstrap_only = 0`.
async fn write_graph_edge(
    store: &SqlxStore,
    source_id: u64,
    target_id: u64,
    weight: f32,
    created_by: &str,
) -> Result<bool> {
    let now = current_unix_seconds();

    let result = sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at,
              created_by, source, bootstrap_only)
         VALUES (?1, ?2, 'Informs', ?3, ?4, ?5, 'behavioral', 0)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(weight)
    .bind(now)
    .bind(created_by)
    .execute(store.write_pool_server())
    .await
    .map_err(|e| StoreError::Database(e.into()))?;

    Ok(result.rows_affected() > 0)
}

/// Current Unix timestamp in seconds (i64 for SQLite binding).
fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// pub(crate) functions
// ---------------------------------------------------------------------------

/// Parse entry IDs from a slice of `ObservationRow`s.
///
/// Filters to `tool = "context_get"` rows only (I-03).
/// Parses the `input` JSON field to extract the integer `id` field.
/// Counts rows where parsing fails (malformed JSON, missing `id`, wrong type, None input).
///
/// Returns `(by_session_id → [(entry_id, ts_millis)], parse_failure_count)`.
/// The `by_session_id` map groups entries by session for co-access pair building.
/// Duplicate entry IDs within the same session are NOT deduplicated here — dedup
/// happens in `build_coaccess_pairs` via the canonical `(min, max)` form.
pub(crate) fn collect_coaccess_entry_ids(
    obs: &[ObservationRow],
) -> (HashMap<String, Vec<(u64, i64)>>, usize) {
    let mut by_session: HashMap<String, Vec<(u64, i64)>> = HashMap::new();
    let mut parse_failures: usize = 0;

    for row in obs {
        // Only process context_get observations (I-03).
        if row.tool.as_deref() != Some("context_get") {
            continue;
        }

        // None input counts as a parse failure.
        let input_json = match row.input.as_deref() {
            Some(s) => s,
            None => {
                parse_failures += 1;
                continue;
            }
        };

        // Parse the JSON.
        let val: serde_json::Value = match serde_json::from_str(input_json) {
            Ok(v) => v,
            Err(_) => {
                parse_failures += 1;
                continue;
            }
        };

        // Extract the integer `id` field.
        let entry_id = match val.get("id").and_then(|v| v.as_u64()) {
            Some(id) => id,
            None => {
                parse_failures += 1;
                continue;
            }
        };

        by_session
            .entry(row.session_id.clone())
            .or_default()
            .push((entry_id, row.ts_millis));
    }

    (by_session, parse_failures)
}

/// Build canonical co-access pairs from per-session entry ID lists.
///
/// Algorithm:
/// 1. For each session, sort entries by `ts_millis` ascending.
/// 2. Enumerate all (i, j) pairs where i < j.
/// 3. Skip self-pairs where `a == b` (DN-3, Resolution 4).
/// 4. Form canonical pair `(min(a,b), max(a,b))` and deduplicate via `HashSet`.
/// 5. Halt enumeration when `pairs.len() == PAIR_CAP` (cap at enumeration time, not truncation).
///
/// Returns `(canonical_pairs, cap_hit)`.
/// `cap_hit` is true when the PAIR_CAP was reached before all pairs were enumerated.
pub(crate) fn build_coaccess_pairs(
    by_session: HashMap<String, Vec<(u64, i64)>>,
) -> (Vec<(u64, u64)>, bool) {
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let mut pairs: Vec<(u64, u64)> = Vec::new();
    let mut cap_hit = false;

    'outer: for (_session_id, mut entries) in by_session {
        // Sort by ts_millis ascending so pairs respect temporal order within session.
        entries.sort_by_key(|(_, ts)| *ts);

        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                let a = entries[i].0;
                let b = entries[j].0;

                // Self-pair exclusion (DN-3) — applied BEFORE dedup.
                if a == b {
                    continue;
                }

                let canonical = (a.min(b), a.max(b));

                // Deduplicate by canonical form.
                if seen.contains(&canonical) {
                    continue;
                }

                seen.insert(canonical);
                pairs.push(canonical);

                // Cap enforced at enumeration time — halt immediately (NFR-04).
                if pairs.len() == PAIR_CAP {
                    cap_hit = true;
                    break 'outer;
                }
            }
        }
    }

    (pairs, cap_hit)
}

/// Map a cycle outcome string to an edge weight.
///
/// `"success"` → 1.0; all others (including `None`) → 0.5.
/// Unknown future outcome strings silently map to 0.5 (R-16).
pub(crate) fn outcome_to_weight(outcome: Option<&str>) -> f32 {
    match outcome {
        Some("success") => 1.0,
        _ => 0.5,
    }
}

/// Emit bidirectional behavioral `Informs` edges for each canonical pair.
///
/// ## write_graph_edge Return Contract (pattern #4041)
///
/// | Return      | Meaning                             | Counter action               |
/// |-------------|-------------------------------------|------------------------------|
/// | `Ok(true)`  | New row inserted (rows_affected==1) | Increment `edges_enqueued`   |
/// | `Ok(false)` | UNIQUE conflict — silent no-op      | Do NOT increment; not error  |
/// | `Err(_)`    | SQL infrastructure failure          | Log `warn!`, do NOT increment |
///
/// For each canonical pair (a, b):
/// - Emits forward edge `(a → b)` and reverse edge `(b → a)`.
/// - `edges_enqueued` increments ONLY on `Ok(true)` — never on `Ok(false)` or `Err`.
/// - `pairs_skipped_on_conflict` increments when BOTH directions return `Ok(false)`.
///
/// Uses `write_pool_server()` directly — NOT the analytics drain (ADR-006 crt-046).
/// `source = 'behavioral'`, `relation_type = 'Informs'`, `bootstrap_only = false`.
///
/// Returns `(edges_enqueued, pairs_skipped_on_conflict)`.
pub(crate) async fn emit_behavioral_edges(
    store: &SqlxStore,
    pairs: &[(u64, u64)],
    weight: f32,
) -> (usize, usize) {
    let mut edges_enqueued: usize = 0;
    let mut pairs_skipped: usize = 0;

    for &(a, b) in pairs {
        // Forward edge: a → b
        let fwd_new = match write_graph_edge(store, a, b, weight, "behavioral_signals").await {
            Ok(true) => {
                edges_enqueued += 1;
                true
            }
            Ok(false) => {
                // UNIQUE conflict — INSERT OR IGNORE silent no-op; do NOT increment (pattern #4041).
                false
            }
            Err(e) => {
                warn!(
                    source_id = a,
                    target_id = b,
                    error = %e,
                    "emit_behavioral_edges: write_graph_edge forward ({a}->{b}) failed"
                );
                // Continue to reverse edge — partial writes are non-fatal.
                // This direction is not counted as a conflict skip.
                // We track whether BOTH directions returned Ok(false) for pairs_skipped.
                // An Err on forward means we cannot determine conflict status; treat as
                // non-conflict (pairs_skipped not incremented).
                continue;
            }
        };

        // Reverse edge: b → a
        let rev_new = match write_graph_edge(store, b, a, weight, "behavioral_signals").await {
            Ok(true) => {
                edges_enqueued += 1;
                true
            }
            Ok(false) => {
                // UNIQUE conflict — INSERT OR IGNORE silent no-op; do NOT increment (pattern #4041).
                false
            }
            Err(e) => {
                warn!(
                    source_id = b,
                    target_id = a,
                    error = %e,
                    "emit_behavioral_edges: write_graph_edge reverse ({b}->{a}) failed"
                );
                continue;
            }
        };

        // A pair is "skipped on conflict" when BOTH directions returned Ok(false).
        // A partial conflict (one new, one conflict) counts as one edge_enqueued, not skipped.
        if !fwd_new && !rev_new {
            pairs_skipped += 1;
        }
    }

    (edges_enqueued, pairs_skipped)
}

/// Write a `goal_clusters` row for a completed feature cycle.
///
/// Serializes `entry_ids` to a JSON array and calls `store.insert_goal_cluster`.
/// INSERT OR IGNORE — returns `Ok(true)` on new row, `Ok(false)` on UNIQUE conflict
/// (first write wins — ADR-002 crt-046).
///
/// This function is the final step in step 8b and is called only after `entry_ids`
/// is fully assembled from `collect_coaccess_entry_ids` (R-06 mitigation).
pub(crate) async fn populate_goal_cluster(
    store: &SqlxStore,
    feature_cycle: &str,
    goal_embedding: Vec<f32>,
    entry_ids: &[u64],
    phase: Option<&str>,
    outcome: Option<&str>,
) -> Result<bool> {
    let entry_ids_json =
        serde_json::to_string(entry_ids).map_err(|e| StoreError::InvalidInput {
            field: "entry_ids".to_string(),
            reason: format!("serde_json::to_string failed: {e}"),
        })?;

    let created_at = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    match store
        .insert_goal_cluster(
            feature_cycle,
            goal_embedding,
            phase,
            &entry_ids_json,
            outcome,
            created_at,
        )
        .await
    {
        Ok(true) => {
            debug!(
                feature_cycle = feature_cycle,
                "populate_goal_cluster: new row inserted"
            );
            Ok(true)
        }
        Ok(false) => {
            debug!(
                feature_cycle = feature_cycle,
                "populate_goal_cluster: UNIQUE conflict — INSERT OR IGNORE no-op (first write wins)"
            );
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// Merge semantic search results and pre-scored cluster entries into a single ranked list.
///
/// PURE FUNCTION — no store access, no async. The caller is responsible for:
/// - Fetching Active entry records via `store.get_by_ids()`.
/// - Computing `cluster_score = (EntryRecord.confidence × w_conf_cluster) + (goal_cosine × w_goal_boost)`.
///   Note: `EntryRecord.confidence` (Wilson-score composite) NOT `IndexEntry.confidence`
///   (raw HNSW cosine). Both compile; the wrong one silently uses cosine twice. (ADR-005 crt-046)
///
/// Algorithm (Option A score-based interleaving — ADR-005 crt-046):
/// 1. Build candidate list: semantic entries use `entry.confidence` as sort score;
///    cluster entries use the pre-computed `cluster_score` f32.
/// 2. Sort descending by score (stable, preserves relative order on ties).
/// 3. Deduplicate by entry ID (first occurrence after sort wins).
/// 4. Return top-k.
///
/// Cold-start (empty `cluster_entries_with_scores`): result is identical to `semantic[:k]`.
pub(crate) fn blend_cluster_entries(
    semantic: Vec<IndexEntry>,
    cluster_entries_with_scores: Vec<(IndexEntry, f32)>,
    k: usize,
) -> Vec<IndexEntry> {
    // Build unified candidate list as (IndexEntry, f64_sort_score).
    let mut candidates: Vec<(IndexEntry, f64)> =
        Vec::with_capacity(semantic.len() + cluster_entries_with_scores.len());

    // Semantic entries: sort score is their existing `confidence` field (raw HNSW cosine).
    for entry in semantic {
        let score = entry.confidence;
        candidates.push((entry, score));
    }

    // Cluster entries: sort score is the pre-computed cluster_score.
    for (entry, cluster_score) in cluster_entries_with_scores {
        candidates.push((entry, cluster_score as f64));
    }

    // Sort descending by score (stable to preserve relative order within equal scores).
    candidates.sort_by(|(_, s1): &(IndexEntry, f64), (_, s2): &(IndexEntry, f64)| {
        s2.partial_cmp(s1).unwrap_or(Ordering::Equal)
    });

    // Deduplicate by entry ID — first occurrence wins (highest score wins after sort).
    let mut seen_ids: HashSet<u64> = HashSet::new();
    let mut result: Vec<IndexEntry> = candidates
        .into_iter()
        .filter(|(entry, _)| seen_ids.insert(entry.id))
        .map(|(entry, _)| entry)
        .collect();

    // Truncate to top-k.
    result.truncate(k);
    result
}

// ---------------------------------------------------------------------------
// Step 8b orchestration
// ---------------------------------------------------------------------------

/// Run the step 8b behavioral signal pipeline for a completed cycle review call.
///
/// Called from the `context_cycle_review` handler on EVERY invocation — both
/// cache-hit (`force=false`) and cache-miss paths (FR-09, Resolution 2, AC-15).
///
/// All errors are non-fatal: logs at `warn!` and returns `parse_failure_count`.
/// The handler must never propagate step 8b errors to the MCP caller.
///
/// Returns `parse_failure_count: u32` — the number of unparseable `context_get`
/// observation rows encountered during `collect_coaccess_entry_ids` (FR-03, R-04).
pub(crate) async fn run_step_8b(
    store: &SqlxStore,
    feature_cycle: &str,
    outcome: Option<&str>,
) -> u32 {
    // Step 1: Load session IDs.
    let session_ids = match store.load_sessions_for_feature(feature_cycle).await {
        Ok(ids) => ids,
        Err(e) => {
            warn!(
                feature_cycle = feature_cycle,
                error = %e,
                "step 8b: load_sessions_for_feature failed — aborting step 8b"
            );
            return 0;
        }
    };

    // Step 2: Load observations.
    let observations = match store.load_observations_for_sessions(&session_ids).await {
        Ok(obs) => obs,
        Err(e) => {
            warn!(
                feature_cycle = feature_cycle,
                error = %e,
                "step 8b: load_observations_for_sessions failed — aborting step 8b"
            );
            return 0;
        }
    };

    // Step 3: Collect co-access entry IDs grouped by session.
    let (by_session, parse_failures) = collect_coaccess_entry_ids(&observations);

    if parse_failures > 0 {
        warn!(
            feature_cycle = feature_cycle,
            parse_failures = parse_failures,
            "step 8b: {} context_get observation(s) failed to parse — entry IDs skipped",
            parse_failures
        );
    }

    // Step 4: Build canonical co-access pairs.
    let (pairs, cap_hit) = build_coaccess_pairs(by_session);

    // Step 5: Log if pair cap was reached.
    if cap_hit {
        warn!(
            feature_cycle = feature_cycle,
            pair_cap = PAIR_CAP,
            "step 8b: pair cap ({}) reached for {} — some pairs not emitted",
            PAIR_CAP,
            feature_cycle
        );
    }

    // Step 6: Determine edge weight from cycle outcome.
    let weight = outcome_to_weight(outcome);

    // Step 7: Emit behavioral edges (skip if no pairs — AC-04).
    if pairs.is_empty() {
        debug!(
            feature_cycle = feature_cycle,
            "step 8b: no co-access pairs for {} — skipping edge emission", feature_cycle
        );
    } else {
        let (enqueued, skipped) = emit_behavioral_edges(store, &pairs, weight).await;
        debug!(
            feature_cycle = feature_cycle,
            edges_enqueued = enqueued,
            pairs_skipped = skipped,
            "step 8b: {} edges enqueued, {} pairs skipped on conflict",
            enqueued,
            skipped
        );
    }

    // Step 8: Get goal embedding.
    let embedding_opt = match store.get_cycle_start_goal_embedding(feature_cycle).await {
        Ok(opt) => opt,
        Err(e) => {
            warn!(
                feature_cycle = feature_cycle,
                error = %e,
                "step 8b: get_cycle_start_goal_embedding failed — skipping goal_cluster write"
            );
            None
        }
    };

    // Step 9: Populate goal cluster if embedding available.
    if let Some(embedding) = embedding_opt {
        // Collect flat union of all entry IDs accessed (across all sessions).
        // Re-call collect_coaccess_entry_ids since by_session was consumed by build_coaccess_pairs.
        let (by_session_2, _) = collect_coaccess_entry_ids(&observations);
        let mut all_entry_ids: Vec<u64> = by_session_2
            .values()
            .flat_map(|v| v.iter().map(|(id, _)| *id))
            .collect();
        all_entry_ids.sort_unstable();
        all_entry_ids.dedup();

        // Determine phase from latest cycle_events row with non-NULL phase.
        let phase_opt = get_latest_cycle_phase(store, feature_cycle).await;

        match populate_goal_cluster(
            store,
            feature_cycle,
            embedding,
            &all_entry_ids,
            phase_opt.as_deref(),
            outcome,
        )
        .await
        {
            Ok(true) => debug!(
                feature_cycle = feature_cycle,
                "step 8b: goal_cluster row written"
            ),
            Ok(false) => debug!(
                feature_cycle = feature_cycle,
                "step 8b: goal_cluster UNIQUE conflict — INSERT OR IGNORE no-op (force=false re-run)"
            ),
            Err(e) => warn!(
                feature_cycle = feature_cycle,
                error = %e,
                "step 8b: populate_goal_cluster failed — continuing"
            ),
        }
    }

    // Step 10: Return parse failures.
    parse_failures as u32
}

/// Query the latest non-NULL phase from `cycle_events` for a cycle.
///
/// Used by `run_step_8b` to populate the `phase` column in `goal_clusters`.
/// Returns `None` on no row, NULL phase, or SQL error (errors logged at warn!).
///
/// Uses `write_pool_server()` — `read_pool()` is crate-private to unimatrix-store.
async fn get_latest_cycle_phase(store: &SqlxStore, cycle_id: &str) -> Option<String> {
    let result = sqlx::query_scalar::<_, String>(
        "SELECT phase FROM cycle_events \
         WHERE cycle_id = ?1 AND phase IS NOT NULL \
         ORDER BY timestamp DESC \
         LIMIT 1",
    )
    .bind(cycle_id)
    .fetch_optional(store.write_pool_server())
    .await;

    match result {
        Ok(phase_opt) => phase_opt,
        Err(e) => {
            warn!(
                cycle_id = cycle_id,
                error = %e,
                "get_latest_cycle_phase: SQL query failed — returning None"
            );
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_obs(session_id: &str, tool: &str, input: Option<&str>, ts: i64) -> ObservationRow {
        ObservationRow {
            id: 0,
            ts_millis: ts,
            hook: "post_tool_call".to_string(),
            session_id: session_id.to_string(),
            tool: Some(tool.to_string()),
            input: input.map(|s| s.to_string()),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_entry(id: u64, confidence: f64) -> IndexEntry {
        IndexEntry {
            id,
            topic: format!("topic-{id}"),
            category: "decision".to_string(),
            confidence,
            snippet: format!("snippet-{id}"),
        }
    }

    // -----------------------------------------------------------------------
    // collect_coaccess_entry_ids
    // -----------------------------------------------------------------------

    #[test]
    fn test_collect_coaccess_entry_ids_extracts_context_get_ids() {
        let obs = vec![
            make_obs("s1", "context_get", Some(r#"{"id": 42}"#), 100),
            make_obs("s1", "context_get", Some(r#"{"id": 43}"#), 200),
            make_obs("s1", "context_get", Some(r#"{"id": 44}"#), 300),
            make_obs("s1", "context_search", Some(r#"{"query": "foo"}"#), 400),
        ];
        let (by_session, failures) = collect_coaccess_entry_ids(&obs);
        assert_eq!(failures, 0);
        let entries = by_session.get("s1").expect("session s1 must be present");
        assert_eq!(entries.len(), 3);
        let ids: Vec<u64> = entries.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&42));
        assert!(ids.contains(&43));
        assert!(ids.contains(&44));
    }

    #[test]
    fn test_collect_coaccess_entry_ids_malformed_json_counted() {
        let obs = vec![
            make_obs("s1", "context_get", Some(r#"{"id": 10}"#), 100),
            make_obs("s1", "context_get", Some(r#"{"id": 11}"#), 200),
            make_obs("s1", "context_get", Some("not-json"), 300),
        ];
        let (by_session, failures) = collect_coaccess_entry_ids(&obs);
        assert_eq!(failures, 1, "one malformed JSON row must be counted");
        let entries = by_session.get("s1").expect("session s1 must be present");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_collect_coaccess_entry_ids_missing_id_field_counted() {
        let obs = vec![make_obs(
            "s1",
            "context_get",
            Some(r#"{"tool": "context_get"}"#),
            100,
        )];
        let (_, failures) = collect_coaccess_entry_ids(&obs);
        assert_eq!(failures, 1, "missing id field must be counted as failure");
    }

    #[test]
    fn test_collect_coaccess_entry_ids_none_input_counted() {
        let obs = vec![make_obs("s1", "context_get", None, 100)];
        let (_, failures) = collect_coaccess_entry_ids(&obs);
        assert_eq!(failures, 1, "None input must be counted as parse failure");
    }

    #[test]
    fn test_collect_coaccess_entry_ids_ignores_non_context_get() {
        let obs = vec![
            make_obs("s1", "context_search", Some(r#"{"query": "foo"}"#), 100),
            make_obs("s1", "context_store", Some(r#"{"content": "bar"}"#), 200),
        ];
        let (by_session, failures) = collect_coaccess_entry_ids(&obs);
        assert!(
            by_session.is_empty(),
            "non-context_get rows must not be added"
        );
        assert_eq!(
            failures, 0,
            "non-context_get rows must not count as failures"
        );
    }

    // -----------------------------------------------------------------------
    // build_coaccess_pairs
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_coaccess_pairs_three_ids_three_pairs() {
        let mut by_session = HashMap::new();
        by_session.insert(
            "s1".to_string(),
            vec![(1u64, 100i64), (2u64, 200i64), (3u64, 300i64)],
        );
        let (pairs, cap_hit) = build_coaccess_pairs(by_session);
        assert_eq!(
            pairs.len(),
            3,
            "3 distinct IDs must produce 3 canonical pairs"
        );
        assert!(!cap_hit);
        // All expected canonical pairs must be present.
        assert!(pairs.contains(&(1, 2)));
        assert!(pairs.contains(&(1, 3)));
        assert!(pairs.contains(&(2, 3)));
    }

    /// E-02 / DN-3: Self-pairs (a == b) must be excluded before dedup.
    #[test]
    fn test_build_coaccess_pairs_self_pairs_excluded() {
        let mut by_session = HashMap::new();
        by_session.insert(
            "s1".to_string(),
            vec![(5u64, 100i64), (5u64, 200i64), (5u64, 300i64)],
        );
        let (pairs, cap_hit) = build_coaccess_pairs(by_session);
        assert!(
            pairs.is_empty(),
            "all same-ID entries must produce zero pairs (self-pair exclusion, DN-3)"
        );
        assert!(!cap_hit);
    }

    #[test]
    fn test_build_coaccess_pairs_cap_enforced_at_200() {
        // 25 distinct IDs → C(25,2) = 300 pairs without cap.
        let mut by_session = HashMap::new();
        let entries: Vec<(u64, i64)> = (1u64..=25).map(|i| (i, i as i64 * 100)).collect();
        by_session.insert("s1".to_string(), entries);
        let (pairs, cap_hit) = build_coaccess_pairs(by_session);
        assert_eq!(
            pairs.len(),
            200,
            "cap must halt enumeration at exactly 200 pairs"
        );
        assert!(cap_hit, "cap_hit must be true when PAIR_CAP was reached");
    }

    #[test]
    fn test_build_coaccess_pairs_no_cap_under_200() {
        // 19 distinct IDs → C(19,2) = 171 pairs < 200.
        let mut by_session = HashMap::new();
        let entries: Vec<(u64, i64)> = (1u64..=19).map(|i| (i, i as i64 * 100)).collect();
        by_session.insert("s1".to_string(), entries);
        let (pairs, cap_hit) = build_coaccess_pairs(by_session);
        assert_eq!(pairs.len(), 171);
        assert!(!cap_hit);
    }

    #[test]
    fn test_build_coaccess_pairs_multi_session_no_cross_session_pairs() {
        let mut by_session = HashMap::new();
        by_session.insert("s1".to_string(), vec![(1u64, 100i64), (2u64, 200i64)]);
        by_session.insert("s2".to_string(), vec![(3u64, 100i64), (4u64, 200i64)]);
        let (pairs, cap_hit) = build_coaccess_pairs(by_session);
        // Expect (1,2) and (3,4) — no cross-session pairs.
        assert!(
            pairs.contains(&(1, 2)),
            "intra-session pair (1,2) must exist"
        );
        assert!(
            pairs.contains(&(3, 4)),
            "intra-session pair (3,4) must exist"
        );
        // Cross-session pairs must not appear.
        assert!(!pairs.contains(&(1, 3)));
        assert!(!pairs.contains(&(1, 4)));
        assert!(!pairs.contains(&(2, 3)));
        assert!(!pairs.contains(&(2, 4)));
        assert!(!cap_hit);
    }

    #[test]
    fn test_build_coaccess_pairs_empty_input_empty_pairs() {
        let (pairs, cap_hit) = build_coaccess_pairs(HashMap::new());
        assert!(pairs.is_empty());
        assert!(!cap_hit);
    }

    #[test]
    fn test_build_coaccess_pairs_single_id_no_pair() {
        let mut by_session = HashMap::new();
        by_session.insert("s1".to_string(), vec![(42u64, 100i64)]);
        let (pairs, cap_hit) = build_coaccess_pairs(by_session);
        assert!(
            pairs.is_empty(),
            "single-entry session must produce no pairs (AC-04)"
        );
        assert!(!cap_hit);
    }

    // -----------------------------------------------------------------------
    // outcome_to_weight
    // -----------------------------------------------------------------------

    #[test]
    fn test_outcome_to_weight_success_returns_1_0() {
        assert_eq!(outcome_to_weight(Some("success")), 1.0f32);
    }

    #[test]
    fn test_outcome_to_weight_none_returns_0_5() {
        assert_eq!(outcome_to_weight(None), 0.5f32);
    }

    #[test]
    fn test_outcome_to_weight_rework_returns_0_5() {
        assert_eq!(outcome_to_weight(Some("rework")), 0.5f32);
    }

    #[test]
    fn test_outcome_to_weight_unknown_returns_0_5() {
        // R-16: future outcome strings silently map to 0.5.
        assert_eq!(
            outcome_to_weight(Some("some-future-outcome-string")),
            0.5f32
        );
    }

    // -----------------------------------------------------------------------
    // emit_behavioral_edges — requires SqlxStore
    // -----------------------------------------------------------------------

    async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = unimatrix_store::test_helpers::open_test_store(&dir).await;
        (store, dir)
    }

    /// R-02-contract: UNIQUE conflict path must NOT increment edges_enqueued.
    ///
    /// Pre-seed both directions of pair (1,2) as NLI Informs edges.
    /// Calling emit_behavioral_edges must return (0, 1): edges_enqueued=0, pairs_skipped=1.
    #[tokio::test]
    async fn test_emit_behavioral_edges_unique_conflict_not_counted() {
        let (store, _dir) = open_test_store().await;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Pre-insert NLI Informs edges for both directions of pair (1,2).
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
             VALUES (1, 2, 'Informs', 1.0, ?1, 'nli', 'nli', 0)",
        )
        .bind(now)
        .execute(store.write_pool_server())
        .await
        .expect("pre-insert forward edge");

        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
             VALUES (2, 1, 'Informs', 1.0, ?1, 'nli', 'nli', 0)",
        )
        .bind(now)
        .execute(store.write_pool_server())
        .await
        .expect("pre-insert reverse edge");

        // Act: emit behavioral edges for the same pair.
        let (enqueued, skipped) = emit_behavioral_edges(&store, &[(1, 2)], 1.0).await;

        assert_eq!(
            enqueued, 0,
            "edges_enqueued must be 0 when UNIQUE conflict (pattern #4041)"
        );
        assert_eq!(
            skipped, 1,
            "pairs_skipped_on_conflict must be 1 when both directions conflict"
        );

        store.close().await.unwrap();
    }

    /// New pair produces both directed edges (R-10).
    #[tokio::test]
    async fn test_emit_behavioral_edges_new_pair_emits_both_directions() {
        let (store, _dir) = open_test_store().await;

        let (enqueued, skipped) = emit_behavioral_edges(&store, &[(1, 2)], 0.5).await;

        assert_eq!(
            enqueued, 2,
            "both directions must be inserted for a new pair"
        );
        assert_eq!(skipped, 0);

        // Verify both rows exist in graph_edges.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM graph_edges
             WHERE source = 'behavioral' AND relation_type = 'Informs'
               AND ((source_id = 1 AND target_id = 2) OR (source_id = 2 AND target_id = 1))",
        )
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
        assert_eq!(count, 2, "both (1->2) and (2->1) rows must exist");

        store.close().await.unwrap();
    }

    /// N pairs → 2N enqueued (R-10).
    #[tokio::test]
    async fn test_emit_behavioral_edges_n_pairs_2n_edges() {
        let (store, _dir) = open_test_store().await;

        let pairs = vec![(1u64, 2u64), (3u64, 4u64), (5u64, 6u64)];
        let (enqueued, skipped) = emit_behavioral_edges(&store, &pairs, 0.5).await;

        assert_eq!(enqueued, 6, "3 pairs must produce 6 directed edges");
        assert_eq!(skipped, 0);

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM graph_edges WHERE source = 'behavioral' AND relation_type = 'Informs'",
        )
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
        assert_eq!(count, 6);

        store.close().await.unwrap();
    }

    /// Weight is stored correctly in graph_edges (AC-03).
    #[tokio::test]
    async fn test_emit_behavioral_edges_weight_stored_in_graph_edge() {
        let (store, _dir) = open_test_store().await;

        emit_behavioral_edges(&store, &[(10, 20)], 1.0).await;

        let weight: f32 = sqlx::query_scalar(
            "SELECT weight FROM graph_edges
             WHERE source_id = 10 AND target_id = 20 AND relation_type = 'Informs'",
        )
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
        assert!(
            (weight - 1.0f32).abs() < f32::EPSILON,
            "weight must be stored as 1.0"
        );

        store.close().await.unwrap();
    }

    /// Empty pairs input → zero edges emitted.
    #[tokio::test]
    async fn test_emit_behavioral_edges_empty_pairs_zero_edges() {
        let (store, _dir) = open_test_store().await;

        let (enqueued, skipped) = emit_behavioral_edges(&store, &[], 1.0).await;
        assert_eq!(enqueued, 0);
        assert_eq!(skipped, 0);

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // populate_goal_cluster — requires SqlxStore
    // -----------------------------------------------------------------------

    fn unit_vec_384() -> Vec<f32> {
        vec![1.0_f32 / (384.0_f32).sqrt(); 384]
    }

    /// Happy path: new row → Ok(true).
    #[tokio::test]
    async fn test_populate_goal_cluster_new_cycle_returns_true() {
        let (store, _dir) = open_test_store().await;
        let embedding = unit_vec_384();

        let result = populate_goal_cluster(
            &store,
            "fc-001",
            embedding,
            &[1, 2, 3],
            Some("impl"),
            Some("success"),
        )
        .await;

        assert!(result.is_ok(), "must return Ok: {:?}", result);
        assert!(result.unwrap(), "new row must return Ok(true)");

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM goal_clusters WHERE feature_cycle = 'fc-001'")
                .fetch_one(store.write_pool_server())
                .await
                .unwrap();
        assert_eq!(count, 1);

        // Verify entry_ids_json round-trips.
        let ids_json: String = sqlx::query_scalar(
            "SELECT entry_ids_json FROM goal_clusters WHERE feature_cycle = 'fc-001'",
        )
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
        let ids: Vec<u64> = serde_json::from_str(&ids_json).expect("must parse back to Vec<u64>");
        assert_eq!(ids, vec![1u64, 2, 3]);

        store.close().await.unwrap();
    }

    /// Duplicate feature_cycle → Ok(false), no error (R-06, INSERT OR IGNORE).
    #[tokio::test]
    async fn test_populate_goal_cluster_duplicate_returns_false() {
        let (store, _dir) = open_test_store().await;
        let embedding = unit_vec_384();

        populate_goal_cluster(&store, "fc-001", embedding.clone(), &[1], None, None)
            .await
            .expect("first insert must succeed");

        let result = populate_goal_cluster(&store, "fc-001", embedding, &[2, 3], None, None).await;

        assert!(result.is_ok(), "duplicate must not error: {:?}", result);
        assert!(
            !result.unwrap(),
            "duplicate feature_cycle must return Ok(false)"
        );

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM goal_clusters")
            .fetch_one(store.write_pool_server())
            .await
            .unwrap();
        assert_eq!(count, 1, "still only one row (first write wins)");

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // blend_cluster_entries — pure function, no store needed
    // -----------------------------------------------------------------------

    /// AC-07: Cluster entry with higher score displaces weakest semantic result.
    ///
    /// FR-21 / ADR-005: Score-based interleaving ensures cluster entries compete
    /// on the same ranked list as semantic results. The weakest semantic entry is
    /// displaced when a cluster entry's cluster_score exceeds it.
    #[test]
    fn test_blend_cluster_entries_displaces_weakest_semantic() {
        // 20 semantic entries with descending scores [1.0, 0.95, ..., 0.05].
        let semantic: Vec<IndexEntry> = (0..20)
            .map(|i| {
                let score = 1.0 - (i as f64 * 0.05);
                make_entry(i as u64 + 100, score)
            })
            .collect();
        let weakest_score = semantic.last().unwrap().confidence;
        let weakest_id = semantic.last().unwrap().id;

        // Cluster entry with score 0.5 — higher than weakest semantic score (~0.05).
        let cluster_entry = make_entry(999, 0.0); // confidence field unused for sort
        let cluster_entries_with_scores = vec![(cluster_entry, 0.5f32)];

        let result = blend_cluster_entries(semantic, cluster_entries_with_scores, 20);

        assert_eq!(result.len(), 20);
        // Cluster entry (id=999, score=0.5) must appear.
        assert!(
            result.iter().any(|e| e.id == 999),
            "cluster entry with score=0.5 must displace weakest semantic (score={weakest_score:.2})"
        );
        // Weakest semantic entry must be displaced.
        assert!(
            !result.iter().any(|e| e.id == weakest_id),
            "weakest semantic entry (id={weakest_id}, score={weakest_score:.2}) must be absent"
        );
    }

    /// Cluster entry with low score → not in top-k (FR-21 / ADR-005 naming-collision guard).
    ///
    /// When cluster_score < all semantic scores, the cluster entry must not displace
    /// any semantic result. This verifies the score-based interleaving is correct
    /// and not a silent no-op (R-13-doc requirement). See ADR-005 crt-046.
    #[test]
    fn test_blend_cluster_entries_low_cluster_score_excluded() {
        // 20 semantic entries with scores [1.0, 0.95, ..., 0.05] — all > 0.10.
        let semantic: Vec<IndexEntry> = (0..20)
            .map(|i| {
                let score = 1.0 - (i as f64 * 0.045);
                make_entry(i as u64 + 100, score)
            })
            .collect();

        // Cluster entry with score 0.10 — below all semantic scores.
        let cluster_entry = make_entry(999, 0.0);
        let cluster_entries_with_scores = vec![(cluster_entry, 0.10f32)];

        let result = blend_cluster_entries(semantic, cluster_entries_with_scores, 20);

        assert_eq!(result.len(), 20, "result must have exactly k entries");
        assert!(
            !result.iter().any(|e| e.id == 999),
            "cluster entry with score 0.10 must not appear when all semantic scores are higher"
        );
    }

    /// Deduplication: entry present in both semantic and cluster lists → appears once.
    #[test]
    fn test_blend_cluster_entries_deduplicates_by_entry_id() {
        // Semantic entry ID=99 with score=0.3.
        let semantic = vec![make_entry(99, 0.3)];
        // Cluster entry ID=99 with higher cluster_score=0.8 — appears first after sort.
        let cluster_entry = make_entry(99, 0.0);
        let cluster_entries_with_scores = vec![(cluster_entry, 0.8f32)];

        let result = blend_cluster_entries(semantic, cluster_entries_with_scores, 5);

        // Entry 99 must appear exactly once.
        let count = result.iter().filter(|e| e.id == 99).count();
        assert_eq!(count, 1, "entry ID 99 must appear exactly once");
        // After sort, cluster entry (score=0.8) appears before semantic (score=0.3).
        // First occurrence wins → the one at position 0 (cluster entry, score=0.8).
        assert_eq!(
            result[0].id, 99,
            "entry 99 must be present (first occurrence after descending sort)"
        );
    }

    /// Cold-start: empty cluster list → result identical to semantic[:k].
    #[test]
    fn test_blend_cluster_entries_empty_cluster_returns_semantic() {
        let semantic: Vec<IndexEntry> = (0..20)
            .map(|i| make_entry(i as u64, 1.0 - (i as f64 * 0.05)))
            .collect();
        let expected_ids: Vec<u64> = semantic.iter().map(|e| e.id).collect();

        let result = blend_cluster_entries(semantic, vec![], 20);

        assert_eq!(result.len(), 20);
        let result_ids: Vec<u64> = result.iter().map(|e| e.id).collect();
        assert_eq!(
            result_ids, expected_ids,
            "empty cluster must return semantic results unchanged"
        );
    }

    /// Returns top-k only.
    #[test]
    fn test_blend_cluster_entries_returns_top_k() {
        let semantic: Vec<IndexEntry> = (0..10)
            .map(|i| make_entry(i as u64 + 100, 0.5 - (i as f64 * 0.01)))
            .collect();
        let cluster: Vec<(IndexEntry, f32)> = (0..5)
            .map(|i| (make_entry(i as u64 + 200, 0.0), 0.9 - (i as f32 * 0.05)))
            .collect();

        let result = blend_cluster_entries(semantic, cluster, 7);
        assert_eq!(result.len(), 7, "result must be exactly k=7 entries");
    }
}
