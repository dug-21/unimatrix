//! Supersession chain handlers for context_graph: chain and current modes (vnc-018).
//!
//! Both modes use SQL recursive CTEs (ADR-001). `find_terminal_active` (in-memory)
//! is PROHIBITED for both modes.
//!
//! Declared as a sub-module of `graph_read.rs` via `#[path]`.

use rmcp::model::ErrorData;
use unimatrix_core::{Status, Store};
use unimatrix_store::{ChainDirection, query_current_terminal, query_supersession_chain};

use crate::error::ERROR_INVALID_PARAMS;

use super::{ChainResult, CurrentResponse, GraphParams, Truncated};

// ---------------------------------------------------------------------------
// chain mode (FR-04, ADR-001)
// ---------------------------------------------------------------------------

/// Walk the supersession chain from `id` using SQL recursive CTEs.
///
/// Returns empty `ChainResult` for non-existent IDs — no error (AC-04).
/// INTENTIONALLY asymmetric with `handle_current` (which returns an error).
/// See R-21 and AC-04. Do not unify these behaviors.
pub(super) async fn handle_chain(
    store: &Store,
    params: &GraphParams,
    id: u64,
) -> Result<ChainResult, ErrorData> {
    // Validate direction for chain mode: forward/backward/both only.
    // "incoming"/"outgoing" are neighbors-mode vocabulary.
    let direction = match params.direction.as_deref().unwrap_or("both") {
        "forward" => ChainDirection::Forward,
        "backward" => ChainDirection::Backward,
        "both" => ChainDirection::Both,
        other => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!(
                    "invalid direction '{other}' for chain mode — chain mode accepts: forward, backward, both"
                ),
                None,
            ));
        }
    };

    // ADR-001: SQL recursive CTE path is mandatory. find_terminal_active is PROHIBITED.
    match query_supersession_chain(store.read_pool_server(), id, direction, 50).await {
        Ok(chain_result) => Ok(ChainResult {
            entries: chain_result.entries,
            truncated: Truncated {
                forward: chain_result.forward_capped,
                backward: chain_result.backward_capped,
            },
        }),
        Err(e) => {
            tracing::error!(id, error = %e, "query_supersession_chain failed");
            Ok(ChainResult {
                entries: vec![],
                truncated: Truncated {
                    forward: false,
                    backward: false,
                },
            })
        }
    }
}

// ---------------------------------------------------------------------------
// current mode (FR-05, ADR-001, R-20)
// ---------------------------------------------------------------------------

/// Follow `superseded_by` from `id` to the terminal Active entry.
///
/// Returns `Err("No active terminal found for entry {id}")` for:
/// - Non-existent ID (AC-05a).
/// - Orphaned deprecated terminal (R-20) — `AND e.status = 0` filter is MANDATORY.
/// - Chain exceeds 50 hops (AC-07).
///
/// INTENTIONALLY asymmetric with `handle_chain`:
/// - `chain` on non-existent ID → empty result (AC-04).
/// - `current` on non-existent ID → error (AC-05a).
/// Asking for the current version of something that doesn't exist is a semantic error,
/// not an empty set. Do NOT unify these behaviors. See R-21.
pub(super) async fn handle_current(store: &Store, id: u64) -> Result<CurrentResponse, String> {
    // ADR-001: SQL recursive CTE path. find_terminal_active (in-memory) is PROHIBITED.
    // query_current_terminal includes `AND e.status = 0` (Active) — guards against orphaned
    // deprecated terminals (R-20 Critical). Without this filter, a deprecated entry with
    // superseded_by IS NULL would be silently returned as the terminal.
    match query_current_terminal(store.read_pool_server(), id).await {
        Ok(Some(entry)) => Ok(CurrentResponse { entry }),
        Ok(None) => {
            // All three failure cases (non-existent ID, orphaned deprecated, chain > 50 hops)
            // produce zero rows at SQL level — same error intentionally (FR-05).
            Err(format!("No active terminal found for entry {id}"))
        }
        Err(e) => {
            tracing::error!(id, error = %e, "query_current_terminal failed");
            Err(format!("No active terminal found for entry {id}"))
        }
    }
}

// ---------------------------------------------------------------------------
// follow_to_current — supersession resolution helper
// ---------------------------------------------------------------------------

/// Follow `superseded_by` from `id` to the terminal Active entry using the store.
///
/// 50-hop safety cap enforced by loop bound.
/// Returns `None` when:
/// - Chain exceeds 50 hops.
/// - Orphaned deprecated terminal (`superseded_by IS NULL`, `status != Active`).
/// - Store error during lookup.
///
/// Caller uses the original ID as a fallback when `None` is returned (ADR-005, R-10).
pub(super) async fn follow_to_current(store: &Store, id: u64) -> Option<u64> {
    let mut current = id;
    for _ in 0..50 {
        let entry = match store.get(current).await {
            Ok(e) => e,
            Err(_) => return None, // Store error — treat as unresolvable.
        };
        match entry.superseded_by {
            None => {
                // Terminal: check status. Active = valid; anything else = orphaned.
                if entry.status == Status::Active {
                    return Some(current);
                } else {
                    // Orphaned deprecated terminal (superseded_by IS NULL, status != Active).
                    // No valid substitution (R-10 edge case).
                    return None;
                }
            }
            Some(next_id) => current = next_id,
        }
    }
    // Loop exhausted: chain exceeds 50 hops.
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::GraphParams;
    use super::*;
    use unimatrix_store::{PoolConfig, SqlxStore, Status};

    async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.db");
        let store = SqlxStore::open(&path, PoolConfig::test_default())
            .await
            .expect("open test store");
        (store, dir)
    }

    async fn insert_entry_direct(
        pool: &sqlx::sqlite::SqlitePool,
        title: &str,
        status: Status,
        supersedes: Option<u64>,
        superseded_by: Option<u64>,
    ) -> u64 {
        let id: i64 =
            sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'next_entry_id'")
                .fetch_one(pool)
                .await
                .expect("get next_entry_id");
        let new_id = id + 1;
        sqlx::query("UPDATE counters SET value = ?1 WHERE name = 'next_entry_id'")
            .bind(new_id)
            .execute(pool)
            .await
            .expect("update counter");

        let status_i = status as i64;
        let now = 1_700_000_000_i64;
        sqlx::query(
            "INSERT INTO entries (id, title, content, topic, category, source, status,
             confidence, created_at, updated_at, last_accessed_at, access_count,
             supersedes, superseded_by, correction_count, embedding_dim,
             created_by, modified_by, content_hash, previous_hash,
             version, feature_cycle, trust_source, helpful_count, unhelpful_count)
             VALUES (?1, ?2, 'content', 'test', 'pattern', 'test', ?3,
             0.5, ?4, ?4, ?4, 0, ?5, ?6, 0, 0, '', '', '', '', 1, '', '', 0, 0)",
        )
        .bind(new_id)
        .bind(title)
        .bind(status_i)
        .bind(now)
        .bind(supersedes.map(|v| v as i64))
        .bind(superseded_by.map(|v| v as i64))
        .execute(pool)
        .await
        .expect("insert entry");

        new_id as u64
    }

    // -----------------------------------------------------------------------
    // handle_chain tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_handle_chain_nonexistent_id_returns_empty() {
        // AC-04, R-21: chain mode returns empty for non-existent ID.
        // INTENTIONALLY asymmetric with current mode which returns error.
        // See R-21 and AC-04. Do not unify these behaviors.
        let (store_impl, _dir) = open_test_store().await;

        let params = GraphParams {
            mode: "chain".to_string(),
            id: Some(999_999),
            ..Default::default()
        };
        let result = handle_chain(&store_impl, &params, 999_999).await.unwrap();

        assert!(
            result.entries.is_empty(),
            "non-existent id must return empty entries"
        );
        assert!(!result.truncated.forward, "forward truncated must be false");
        assert!(
            !result.truncated.backward,
            "backward truncated must be false"
        );
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_chain_five_entry_chain_both_directions() {
        // AC-01: five-entry chain queried from middle returns all 5 ordered oldest→newest.
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        // Create chain: A → B → C → D → E (A oldest, E newest).
        let a = insert_entry_direct(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry_direct(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry_direct(wp, "C", Status::Deprecated, Some(b), None).await;
        let d = insert_entry_direct(wp, "D", Status::Deprecated, Some(c), None).await;
        let e = insert_entry_direct(wp, "E", Status::Active, Some(d), None).await;

        // Set superseded_by links.
        for (old, new) in [(a, b), (b, c), (c, d), (d, e)] {
            sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
                .bind(new as i64)
                .bind(old as i64)
                .execute(wp)
                .await
                .unwrap();
        }

        let params = GraphParams {
            mode: "chain".to_string(),
            id: Some(c),
            direction: Some("both".to_string()),
            ..Default::default()
        };
        let result = handle_chain(&store_impl, &params, c).await.unwrap();

        assert_eq!(result.entries.len(), 5, "all 5 entries must be returned");
        let ids: Vec<u64> = result.entries.iter().map(|entry| entry.id).collect();
        let pos_a = ids.iter().position(|&x| x == a).unwrap();
        let pos_c = ids.iter().position(|&x| x == c).unwrap();
        let pos_e = ids.iter().position(|&x| x == e).unwrap();
        assert!(pos_a < pos_c, "A must come before C");
        assert!(pos_c < pos_e, "C must come before E");
        assert!(!result.truncated.forward);
        assert!(!result.truncated.backward);
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_chain_direction_forward_from_mid_chain() {
        // AC-02: forward direction from mid-chain returns seed + descendants.
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let a = insert_entry_direct(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry_direct(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry_direct(wp, "C", Status::Active, Some(b), None).await;

        for (old, new) in [(a, b), (b, c)] {
            sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
                .bind(new as i64)
                .bind(old as i64)
                .execute(wp)
                .await
                .unwrap();
        }

        let params = GraphParams {
            mode: "chain".to_string(),
            id: Some(a),
            direction: Some("forward".to_string()),
            ..Default::default()
        };
        let result = handle_chain(&store_impl, &params, a).await.unwrap();

        let ids: Vec<u64> = result.entries.iter().map(|entry| entry.id).collect();
        assert!(ids.contains(&a), "seed A must be included");
        assert!(ids.contains(&b), "B must be in forward result");
        assert!(ids.contains(&c), "C must be in forward result");
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_chain_invalid_direction_returns_error() {
        // "incoming" is neighbors-mode vocabulary and must be rejected by chain mode
        // with a proper error — not silently swallowed as an empty result.
        let (store_impl, _dir) = open_test_store().await;

        let params = GraphParams {
            mode: "chain".to_string(),
            id: Some(1),
            direction: Some("incoming".to_string()),
            ..Default::default()
        };
        let result = handle_chain(&store_impl, &params, 1).await;

        assert!(result.is_err(), "invalid direction must return Err");
        let err = result.unwrap_err();
        let msg: &str = &err.message;
        assert!(
            msg.contains("chain"),
            "error must mention 'chain', got: {msg}"
        );
        assert!(
            msg.contains("forward"),
            "error must mention 'forward', got: {msg}"
        );
        assert!(
            msg.contains("backward"),
            "error must mention 'backward', got: {msg}"
        );
        assert!(
            msg.contains("both"),
            "error must mention 'both', got: {msg}"
        );
        store_impl.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // handle_current tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_handle_current_active_entry_returns_self() {
        // AC-05: current on active entry returns same entry.
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let id = insert_entry_direct(wp, "Active Entry", Status::Active, None, None).await;

        let result = handle_current(&store_impl, id).await;
        assert!(result.is_ok(), "active entry must return Ok");
        let resp = result.unwrap();
        assert_eq!(resp.entry.id, id);
        assert_eq!(resp.entry.status, Status::Active);
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_current_nonexistent_id_returns_error() {
        // AC-05a, R-21: current on non-existent ID returns error.
        // INTENTIONALLY asymmetric with chain mode (returns empty for same ID).
        // This asymmetry is correct by design — current is a lookup that must
        // succeed or fail, not a traversal that can return empty. See R-21.
        let (store_impl, _dir) = open_test_store().await;

        let result = handle_current(&store_impl, 999_999).await;
        assert!(result.is_err(), "non-existent id must return error");
        let msg = result.unwrap_err();
        assert!(
            msg.to_lowercase().contains("no active terminal"),
            "error must mention 'no active terminal', got: {msg}"
        );
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_current_deprecated_resolves_to_active_terminal() {
        // AC-06: deprecated entry with valid chain resolves to active terminal.
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let a = insert_entry_direct(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry_direct(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry_direct(wp, "C", Status::Active, Some(b), None).await;

        for (old, new) in [(a, b), (b, c)] {
            sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
                .bind(new as i64)
                .bind(old as i64)
                .execute(wp)
                .await
                .unwrap();
        }

        let result = handle_current(&store_impl, a).await;
        assert!(
            result.is_ok(),
            "deprecated entry must resolve to active terminal"
        );
        assert_eq!(result.unwrap().entry.id, c);
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_current_orphaned_deprecated_returns_error() {
        // AC-06b, R-20: orphaned deprecated entry (superseded_by IS NULL, status=Deprecated)
        // must return error — NOT the deprecated entry as the terminal.
        // COMMENT: This is the only test that catches an accidentally omitted
        // `AND e.status = 0` (Active) filter in the CTE. Without this filter,
        // the deprecated entry would be returned as if it were an active terminal (R-20 Critical).
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let id =
            insert_entry_direct(wp, "Orphaned Deprecated", Status::Deprecated, None, None).await;

        let result = handle_current(&store_impl, id).await;
        assert!(
            result.is_err(),
            "orphaned deprecated entry must return error, not the entry itself"
        );
        store_impl.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // follow_to_current tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_follow_to_current_active_entry_returns_self() {
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let id = insert_entry_direct(wp, "Active", Status::Active, None, None).await;
        let result = follow_to_current(&store_impl, id).await;
        assert_eq!(result, Some(id), "active entry must resolve to itself");
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_follow_to_current_chain_resolves() {
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let a = insert_entry_direct(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry_direct(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry_direct(wp, "C", Status::Active, Some(b), None).await;

        for (old, new) in [(a, b), (b, c)] {
            sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
                .bind(new as i64)
                .bind(old as i64)
                .execute(wp)
                .await
                .unwrap();
        }

        let result = follow_to_current(&store_impl, a).await;
        assert_eq!(result, Some(c), "chain must resolve to terminal active C");
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_follow_to_current_orphaned_returns_none() {
        // R-10: orphaned deprecated entry (superseded_by IS NULL, status=Deprecated) → None.
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let id =
            insert_entry_direct(wp, "Orphaned Deprecated", Status::Deprecated, None, None).await;
        let result = follow_to_current(&store_impl, id).await;
        assert!(
            result.is_none(),
            "orphaned deprecated entry must return None"
        );
        store_impl.close().await.unwrap();
    }
}
