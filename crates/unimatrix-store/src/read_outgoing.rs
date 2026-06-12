//! Outgoing-edge read query for `context_correct` carry-forward (vnc-035).
//!
//! This module hosts `query_outgoing_edges` and its `OutgoingEdgeRow` DTO. It is a
//! sibling of `query_incoming_edges` (`read.rs:1694`); it lives in its own module
//! because `read.rs` already exceeds the 500-line housekeeping rule (O-2) and the
//! 500-line limit forbids growing it further.
//!
//! The query is the **single source of truth** for the outgoing eligibility
//! predicate (NFR-02 / SR-03 / ADR-002 vnc-035). The exclusion list is expressed
//! ONCE, at the SQL level — no parallel Rust-side filter exists or may be added.

use sqlx::Row;

use crate::db::SqlxStore;
use crate::error::{Result, StoreError};

/// One eligible outgoing `graph_edges` row returned by `query_outgoing_edges` (vnc-035).
///
/// `source_id` is the query parameter and is implicit; it is not included in the
/// struct (mirrors `IncomingEdgeRow`, which omits the implicit `target_id`).
///
/// `created_at` is read for ordering / observability ONLY. The carry-forward
/// RE-STAMPS `created_at = now` (the correction timestamp); the source row's value
/// is NOT written onto the new entry B (ADR-004 / FR-11). Reading it keeps the DTO
/// symmetric with `IncomingEdgeRow` and available for deterministic loop ordering.
#[derive(Debug, Clone)]
pub struct OutgoingEdgeRow {
    /// Entry ID of the target (the entry A declared an edge toward).
    pub target_id: u64,
    /// Relation type string as stored (e.g. `"Supports"`, `"Advances"`, `"Contradicts"`).
    pub relation_type: String,
    /// Unix timestamp (seconds) when the original edge was created.
    /// NOT written onto B — see struct doc (ADR-004).
    pub created_at: u64,
}

impl SqlxStore {
    /// Return all eligible **outgoing** `graph_edges` rows from `source_id`.
    ///
    /// Used by `context_correct`'s carry-forward loop (vnc-035, `run_carry_forward_loop`)
    /// to discover the original entry A's agent-declared outgoing edges before copying
    /// them onto the new corrected entry B.
    ///
    /// # Eligibility predicate — SINGLE SOURCE OF TRUTH (SR-03, ADR-002 vnc-035)
    ///
    /// Eligible = **agent-declared edges only**. Derived / tick-generated classes are
    /// excluded at the SQL level (`NOT IN ('Supersedes', 'CoAccess', 'Informs')`); they
    /// re-materialize on their own and must not be carried.
    ///
    /// This is a **SUPERSET** of `query_incoming_edges`' exclusion, which drops ONLY
    /// `'Supersedes'`. The difference is INTENTIONAL, not drift: `CoAccess`/`Informs`
    /// are outgoing-relevant from a hub entry but not incoming-relevant to a correction
    /// target, so the incoming query has no reason to exclude them. Do NOT "align" the
    /// two predicates into false symmetry — doing so would silently carry tick-generated
    /// classes (R-03). See ADR-002 vnc-035.
    ///
    /// # No outgoing ceiling
    /// Every eligible edge always carries (AC-09). The absence of a ceiling is safe
    /// ONLY while eligibility = agent-declared-only — this predicate is what bounds
    /// agent-declared out-degree (SR-04).
    ///
    /// # Pool
    /// Uses `read_pool()`. Both `read_pool()` and `write_pool_server()` currently alias
    /// the same underlying pool (`db.rs:294`); use canonical accessor name per C-07.
    ///
    /// # Index
    /// `idx_graph_edges_source_id` covers `WHERE source_id = ?` efficiently
    /// (`db.rs:969`, `migration.rs:367`) — no full-table scan (R-09 resolved at plan time).
    ///
    /// # Errors
    /// SQL/pool failure or per-row decode failure → `Err(StoreError::Database(..))`.
    /// The query never warns or swallows; it propagates `Err` to its caller, which owns
    /// the warn-and-continue posture (ADR-002). Mirrors `query_incoming_edges`.
    pub async fn query_outgoing_edges(&self, source_id: u64) -> Result<Vec<OutgoingEdgeRow>> {
        let rows = sqlx::query(
            "SELECT target_id, relation_type, created_at \
             FROM graph_edges \
             WHERE source_id = ?1 \
               AND relation_type NOT IN ('Supersedes', 'CoAccess', 'Informs')
               -- ELIGIBILITY PREDICATE — SINGLE SOURCE OF TRUTH (SR-03, ADR-002 vnc-035).
               -- Agent-declared edges carry forward on correction; derived/tick-generated
               -- classes do NOT (they re-materialize on their own):
               --   'Supersedes' — derived from entries.supersedes; rebuilt by the graph tick.
               --   'CoAccess'   — tick-generated co-access affinity; re-promoted by its own tick.
               --   'Informs'    — tick-generated affinity class; re-materializes.
               -- SUPERSET vs query_incoming_edges (which excludes ONLY 'Supersedes'):
               -- this is INTENTIONAL, NOT drift. CoAccess/Informs are OUTGOING-relevant from a
               -- hub entry but NOT incoming-relevant to a correction target, so the incoming
               -- query has no reason to exclude them. Do NOT 'align' the two predicates into
               -- false symmetry — doing so would silently carry tick-generated classes (R-03).
               -- See ADR-002 vnc-035.",
        )
        // SQLite stores IDs as i64 (BIGINT); cast u64 → i64 for binding, matching the
        // pattern used throughout read.rs (query_incoming_edges, query_graph_edges).
        .bind(source_id as i64)
        // read_pool() and write_pool_server() currently alias the same underlying pool
        // (db.rs:294). Use canonical accessor name per C-07.
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.into_iter()
            .map(|row| {
                Ok(OutgoingEdgeRow {
                    target_id: row
                        .try_get::<i64, _>("target_id")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    relation_type: row
                        .try_get("relation_type")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    created_at: row
                        .try_get::<i64, _>("created_at")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                })
            })
            .collect::<Result<Vec<_>>>()
    }
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::open_test_store;

    /// Create the `graph_edges` table for tests that run against a pre-v13 schema.
    /// Mirrors the helper in `read.rs` tests (cumulative test infra — NFR-06).
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

    /// Insert a `graph_edges` row using only the columns needed for
    /// `query_outgoing_edges` tests. Other columns use schema defaults.
    async fn insert_graph_edge_minimal(
        pool: &sqlx::sqlite::SqlitePool,
        source_id: i64,
        target_id: i64,
        relation_type: &str,
        created_at: i64,
    ) {
        sqlx::query(
            "INSERT INTO graph_edges \
                 (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) \
             VALUES (?1, ?2, ?3, 1.0, ?4, 'agent', 'manual', 0)",
        )
        .bind(source_id)
        .bind(target_id)
        .bind(relation_type)
        .bind(created_at)
        .execute(pool)
        .await
        .expect("insert graph_edge");
    }

    /// `test_query_outgoing_excludes_derived_classes` (R-03, AC-04 unit half) — REQUIRED.
    ///
    /// Seed A with `Supports` (eligible) + `Supersedes` + `CoAccess` + `Informs`.
    /// Assert ONLY the `Supports` row returns; the three derived/tick classes are
    /// excluded. Pins the exclusion set `('Supersedes','CoAccess','Informs')`.
    #[tokio::test]
    async fn test_query_outgoing_excludes_derived_classes() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        insert_graph_edge_minimal(&store.write_pool, 99, 10, "Supports", 1000).await;
        insert_graph_edge_minimal(&store.write_pool, 99, 20, "Supersedes", 2000).await;
        insert_graph_edge_minimal(&store.write_pool, 99, 30, "CoAccess", 3000).await;
        insert_graph_edge_minimal(&store.write_pool, 99, 40, "Informs", 4000).await;

        let rows = store
            .query_outgoing_edges(99)
            .await
            .expect("query_outgoing_edges");

        assert_eq!(
            rows.len(),
            1,
            "only the agent-declared Supports row is eligible; the 3 derived/tick classes \
             (Supersedes, CoAccess, Informs) must be excluded at SQL level"
        );
        assert_eq!(rows[0].relation_type, "Supports");
        assert_eq!(rows[0].target_id, 10);
    }

    /// `test_query_outgoing_returns_eligible_with_fields` (AC-01 store half) — REQUIRED.
    ///
    /// Seed A with two eligible outgoing edges at known timestamps; assert both rows'
    /// `target_id`/`relation_type` match the seeded triples and `created_at` reflects
    /// the stored value (read-only — NOT written onto B per ADR-004).
    #[tokio::test]
    async fn test_query_outgoing_returns_eligible_with_fields() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        insert_graph_edge_minimal(&store.write_pool, 99, 111, "Supports", 1000).await;
        insert_graph_edge_minimal(&store.write_pool, 99, 222, "Advances", 2000).await;

        let rows = store
            .query_outgoing_edges(99)
            .await
            .expect("query_outgoing_edges");

        assert_eq!(rows.len(), 2, "expected exactly 2 eligible outgoing rows");

        let supports = rows
            .iter()
            .find(|r| r.relation_type == "Supports")
            .expect("Supports row");
        assert_eq!(supports.target_id, 111);
        assert_eq!(supports.created_at, 1000);

        let advances = rows
            .iter()
            .find(|r| r.relation_type == "Advances")
            .expect("Advances row");
        assert_eq!(advances.target_id, 222);
        assert_eq!(advances.created_at, 2000);
    }

    /// `test_query_outgoing_empty_when_no_edges` (R-02 zero-carry support) — REQUIRED.
    ///
    /// Seed A with no outgoing edges, plus one **incoming** edge `E→A` to prove
    /// directionality. Assert `Ok(vec![])` — the incoming edge does not leak into the
    /// outgoing result (confirms `WHERE source_id = ?1`, not `target_id`).
    #[tokio::test]
    async fn test_query_outgoing_empty_when_no_edges() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        // Incoming edge E(50) -> A(99): A is the TARGET, not the source.
        insert_graph_edge_minimal(&store.write_pool, 50, 99, "Supports", 1000).await;

        let rows = store
            .query_outgoing_edges(99)
            .await
            .expect("query_outgoing_edges");

        assert!(
            rows.is_empty(),
            "A has no outgoing edges; the incoming edge E->A must not leak into the \
             outgoing result (WHERE source_id = ?1, not target_id)"
        );
    }

    /// `test_query_outgoing_only_ineligible_returns_empty` (edge case) — REQUIRED.
    ///
    /// Seed A with only `Supersedes` + `CoAccess` rows. Assert `Ok(vec![])`: raw rows
    /// exist but all are excluded → empty eligible set. Supports the handler-level
    /// "ineligible-only → carried == 0, ack omitted" edge case.
    #[tokio::test]
    async fn test_query_outgoing_only_ineligible_returns_empty() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        insert_graph_edge_minimal(&store.write_pool, 99, 10, "Supersedes", 1000).await;
        insert_graph_edge_minimal(&store.write_pool, 99, 20, "CoAccess", 2000).await;

        let rows = store
            .query_outgoing_edges(99)
            .await
            .expect("query_outgoing_edges");

        assert!(
            rows.is_empty(),
            "all seeded rows are ineligible (Supersedes, CoAccess); eligible set is empty"
        );
    }
}
