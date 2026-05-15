//! Edge validation and write helpers for agent-declared graph edges.
//!
//! Extracted from `tools.rs` per ADR-005 (500-line rule). Provides the public
//! surface consumed by `context_store`, `context_correct`, and `context_edge`
//! handlers.
//!
//! # Three-Case Contract for `write_graph_edge` (Pattern #4041)
//!
//! Every call to `write_graph_edge` must be interpreted as follows:
//! - `true`  — row was inserted (`rows_affected = 1`); continue
//! - `false` — `INSERT OR IGNORE` hit UNIQUE constraint; idempotent; not an error; continue
//! - `false` (from Err) — SQL error logged inside `write_graph_edge`; do not surface to caller
//!
//! Never treat `false` as a hard failure or abort the loop on it.

use fmt::Display;
use std::fmt;

use unimatrix_core::{Status, Store};
use unimatrix_engine::graph::RelationType;
use unimatrix_store::StoreError;

use crate::mcp::tools::EdgeInput;
use crate::services::nli_detection::write_graph_edge;

/// Source tag for `GRAPH_EDGES.source` AND `GRAPH_EDGES.created_by` on all
/// agent-declared edges (ADR-008). Analogous to `EDGE_SOURCE_NLI`.
pub(crate) const EDGE_SOURCE_AGENT: &str = "agent";

/// Errors that abort the edge validation pipeline before any write occurs.
///
/// Returned by `validate_and_write_edges` (and `validate_target`). Any variant
/// means the entire call fails — no entry is written, no edges are written.
#[derive(Debug)]
pub(crate) enum EdgeValidationError {
    /// `edge_type` string could not be resolved to a known `RelationType`.
    UnknownType { edge_type: String },
    /// `source_id == target_id` — self-referential edge is not permitted.
    SelfReferential { id: u64 },
    /// Target entry does not exist (or store returned an error).
    TargetNotFound { target_id: u64 },
    /// Target entry exists but is quarantined — cannot be referenced.
    TargetQuarantined { target_id: u64 },
}

impl Display for EdgeValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EdgeValidationError::UnknownType { edge_type } => {
                write!(f, "unknown edge type '{edge_type}'")
            }
            EdgeValidationError::SelfReferential { id } => {
                write!(
                    f,
                    "self-referential edge: source and target are both entry {id}"
                )
            }
            EdgeValidationError::TargetNotFound { target_id } => {
                write!(f, "target entry {target_id} does not exist")
            }
            EdgeValidationError::TargetQuarantined { target_id } => {
                write!(
                    f,
                    "target entry {target_id} is quarantined and cannot be referenced"
                )
            }
        }
    }
}

/// Infrastructure error from `delete_graph_edge`.
///
/// A zero-row DELETE is NOT an error (idempotent). Only a pool/SQL failure
/// produces this variant.
// Used by context_edge handler (Component 8 — not yet implemented).
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum EdgeDeleteError {
    StoreError(StoreError),
}

impl Display for EdgeDeleteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EdgeDeleteError::StoreError(e) => write!(f, "delete_graph_edge store error: {e}"),
        }
    }
}

/// Error from `redirect_graph_edge` (atomic transaction).
///
/// On any SQL or transaction error the `sqlx::Transaction` is dropped, which
/// triggers an automatic ROLLBACK (lesson #2269 — RAII transaction pattern).
// Used by context_edge handler (Component 8 — not yet implemented).
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum EdgeRedirectError {
    /// `new_target_id` does not exist.
    TargetNotFound { target_id: u64 },
    /// `new_target_id` is quarantined.
    TargetQuarantined { target_id: u64 },
    /// SQLite transaction error (begin, execute, or commit).
    TransactionError(sqlx::Error),
}

impl Display for EdgeRedirectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EdgeRedirectError::TargetNotFound { target_id } => {
                write!(f, "redirect target entry {target_id} does not exist")
            }
            EdgeRedirectError::TargetQuarantined { target_id } => {
                write!(f, "redirect target entry {target_id} is quarantined")
            }
            EdgeRedirectError::TransactionError(e) => {
                write!(f, "redirect_graph_edge transaction error: {e}")
            }
        }
    }
}

/// Check that `target_id` refers to an existing, non-quarantined entry.
///
/// - `Active` → allowed
/// - `Deprecated` → allowed (DependencyOnDeprecated rule surfaces these)
/// - `Quarantined` → `Err(TargetQuarantined)`
/// - Not found / store error → `Err(TargetNotFound)`
async fn validate_target(store: &Store, target_id: u64) -> Result<(), EdgeValidationError> {
    match store.get(target_id).await {
        Ok(entry) => {
            if entry.status == Status::Quarantined {
                Err(EdgeValidationError::TargetQuarantined { target_id })
            } else {
                Ok(())
            }
        }
        Err(_) => Err(EdgeValidationError::TargetNotFound { target_id }),
    }
}

/// Validate a slice of agent-declared edges and write them.
///
/// Called **post-insert only** — `source_id` is the actual assigned ID.
/// Phase A in the handler validates types and targets pre-insert;
/// this function re-validates and adds the self-ref check (Phase B).
///
/// Checks per edge: type resolution → self-ref → target (quarantine/exists).
/// First failure returns immediately; no writes on any failure.
///
/// `Contradicts` edges write both (A→B) and (B→A) before returning (AC-06).
/// Infrastructure failures from `write_graph_edge` are logged inside that
/// function; the entry is never rolled back (ADR-003).
pub(crate) async fn validate_and_write_edges(
    store: &Store,
    source_id: u64,
    edges: &[EdgeInput],
    created_at: u64,
) -> Result<(), EdgeValidationError> {
    if edges.is_empty() {
        return Ok(());
    }

    // ── THREE-CASE CONTRACT FOR write_graph_edge (Pattern #4041) ─────────────
    // write_graph_edge returns bool, NOT Result<bool, _>:
    //   true  → row inserted (rows_affected = 1)
    //   false → INSERT OR IGNORE hit UNIQUE constraint — already exists (idempotent, not error)
    //   [Err is handled INSIDE write_graph_edge and logged there; caller receives false]
    // DO NOT treat false as an error. DO NOT surface false to the MCP caller.
    // ─────────────────────────────────────────────────────────────────────────

    // Phase A: type resolution + self-ref + target validation (all before any write)
    let mut resolved: Vec<(RelationType, u64)> = Vec::with_capacity(edges.len());

    for edge in edges {
        // 1. Edge type resolution (pure — no DB)
        let rel_type = RelationType::from_str(&edge.edge_type).ok_or_else(|| {
            EdgeValidationError::UnknownType {
                edge_type: edge.edge_type.clone(),
            }
        })?;

        // 2. Self-referential check (source_id is the actual post-insert id)
        if source_id == edge.target_id {
            return Err(EdgeValidationError::SelfReferential { id: source_id });
        }

        // 3. Target validation (1 DB read per edge via read_pool)
        validate_target(store, edge.target_id).await?;

        resolved.push((rel_type, edge.target_id));
    }

    // Phase B: write loop — all edges passed validation
    for (rel_type, target_id) in resolved {
        // Three-case contract applies: false from UNIQUE conflict is not an error
        let _inserted = write_graph_edge(
            store,
            source_id,
            target_id,
            rel_type.as_str(),
            1.0,
            created_at,
            EDGE_SOURCE_AGENT,
            "",
        )
        .await;

        // Bidirectional Contradicts: write reverse direction (AC-06, ADR-003)
        // Fire-and-forget sequential — not transactional.
        // If the reverse write fails the forward direction persists (graph is asymmetric
        // until repaired). This is the accepted partial-write posture (ADR-003).
        if rel_type == RelationType::Contradicts {
            let _inserted_reverse = write_graph_edge(
                store,
                target_id, // reversed: target becomes source
                source_id, // reversed: source becomes target
                "Contradicts",
                1.0,
                created_at,
                EDGE_SOURCE_AGENT,
                "",
            )
            .await;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// delete_graph_edge
// ---------------------------------------------------------------------------

/// Delete a single graph edge by `(source_id, target_id, relation_type)` triplet.
///
/// Idempotent: zero rows affected is success (AC-25, NFR-03).
///
/// For `Contradicts`, both directions are deleted in sequence before returning.
// Used by context_edge handler (Component 8 — not yet implemented).
#[allow(dead_code)]
pub(crate) async fn delete_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
) -> Result<(), EdgeDeleteError> {
    let pool = store.write_pool_server();

    // Delete primary direction
    sqlx::query(
        "DELETE FROM graph_edges \
         WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(relation_type)
    .execute(pool)
    .await
    .map_err(|e| EdgeDeleteError::StoreError(StoreError::Database(e.into())))?;

    // rows_affected = 0 is NOT an error — idempotent delete (AC-25)

    // Bidirectional Contradicts: delete the reverse direction
    if relation_type == "Contradicts" {
        sqlx::query(
            "DELETE FROM graph_edges \
             WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3",
        )
        .bind(target_id as i64) // reversed
        .bind(source_id as i64) // reversed
        .bind("Contradicts")
        .execute(pool)
        .await
        .map_err(|e| EdgeDeleteError::StoreError(StoreError::Database(e.into())))?;
        // 0 rows affected on reverse is still success (idempotent)
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// redirect_graph_edge
// ---------------------------------------------------------------------------

/// Atomically redirect an edge from `old_target_id` to `new_target_id`.
///
/// Uses a RAII `sqlx::Transaction` — **not raw BEGIN/COMMIT SQL strings**.
/// This is mandatory: lesson #2269 confirms that raw SQL transaction strings
/// lose atomicity under `write_max_connections >= 2` because the pool may
/// allocate different connections for each statement.
///
/// # Transaction scope
/// - Non-`Contradicts`: 2 SQL statements (DELETE old, INSERT new)
/// - `Contradicts`: 4 SQL statements (DELETE A→B, DELETE B→A, INSERT A→B', INSERT B'→A)
///
/// All statements execute against `&mut *txn` — same connection.
/// Dropping `txn` without calling `commit()` triggers automatic ROLLBACK.
///
/// # Caller contract
/// Target validation for `new_target_id` (existence + quarantine check) MUST be
/// performed by the caller **before** invoking this function. This function trusts
/// the validated caller and performs no re-validation.
// Used by context_edge handler (Component 8 — not yet implemented).
#[allow(dead_code)]
pub(crate) async fn redirect_graph_edge(
    store: &Store,
    source_id: u64,
    old_target_id: u64,
    new_target_id: u64,
    relation_type: &str,
    created_at: u64,
) -> Result<(), EdgeRedirectError> {
    // CRITICAL: RAII transaction — all statements on same connection (lesson #2269)
    let pool = store.write_pool_server();
    let mut txn = pool
        .begin()
        .await
        .map_err(EdgeRedirectError::TransactionError)?;

    if relation_type == "Contradicts" {
        // ── Contradicts: 4-row atomic operation ─────────────────────────────

        // 1. Delete A→B (old forward direction)
        sqlx::query(
            "DELETE FROM graph_edges \
             WHERE source_id = ?1 AND target_id = ?2 AND relation_type = 'Contradicts'",
        )
        .bind(source_id as i64)
        .bind(old_target_id as i64)
        .execute(&mut *txn)
        .await
        .map_err(EdgeRedirectError::TransactionError)?;

        // 2. Delete B→A (old reverse direction)
        sqlx::query(
            "DELETE FROM graph_edges \
             WHERE source_id = ?1 AND target_id = ?2 AND relation_type = 'Contradicts'",
        )
        .bind(old_target_id as i64)
        .bind(source_id as i64)
        .execute(&mut *txn)
        .await
        .map_err(EdgeRedirectError::TransactionError)?;

        // 3. Insert A→B' (new forward direction; INSERT OR IGNORE for idempotency)
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges \
             (source_id, target_id, relation_type, weight, created_at, created_by, source, \
              bootstrap_only, metadata) \
             VALUES (?1, ?2, 'Contradicts', 1.0, ?3, ?4, ?4, 0, '')",
        )
        .bind(source_id as i64)
        .bind(new_target_id as i64)
        .bind(created_at as i64)
        .bind(EDGE_SOURCE_AGENT)
        .execute(&mut *txn)
        .await
        .map_err(EdgeRedirectError::TransactionError)?;

        // 4. Insert B'→A (new reverse direction)
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges \
             (source_id, target_id, relation_type, weight, created_at, created_by, source, \
              bootstrap_only, metadata) \
             VALUES (?1, ?2, 'Contradicts', 1.0, ?3, ?4, ?4, 0, '')",
        )
        .bind(new_target_id as i64)
        .bind(source_id as i64)
        .bind(created_at as i64)
        .bind(EDGE_SOURCE_AGENT)
        .execute(&mut *txn)
        .await
        .map_err(EdgeRedirectError::TransactionError)?;
    } else {
        // ── Non-Contradicts: 2-row atomic operation ──────────────────────────

        // 1. Delete old edge
        sqlx::query(
            "DELETE FROM graph_edges \
             WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3",
        )
        .bind(source_id as i64)
        .bind(old_target_id as i64)
        .bind(relation_type)
        .execute(&mut *txn)
        .await
        .map_err(EdgeRedirectError::TransactionError)?;

        // 2. Insert new edge (INSERT OR IGNORE for idempotency)
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges \
             (source_id, target_id, relation_type, weight, created_at, created_by, source, \
              bootstrap_only, metadata) \
             VALUES (?1, ?2, ?3, 1.0, ?4, ?5, ?5, 0, '')",
        )
        .bind(source_id as i64)
        .bind(new_target_id as i64)
        .bind(relation_type)
        .bind(created_at as i64)
        .bind(EDGE_SOURCE_AGENT)
        .execute(&mut *txn)
        .await
        .map_err(EdgeRedirectError::TransactionError)?;
    }

    // Commit: dropping txn without commit triggers automatic ROLLBACK (RAII)
    txn.commit()
        .await
        .map_err(EdgeRedirectError::TransactionError)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_source_agent_constant_value() {
        assert_eq!(EDGE_SOURCE_AGENT, "agent");
    }

    #[test]
    fn test_edge_source_agent_distinctness() {
        assert_ne!(EDGE_SOURCE_AGENT, "");
        assert_ne!(EDGE_SOURCE_AGENT, "system");
        assert_ne!(EDGE_SOURCE_AGENT, "human");
        assert_ne!(EDGE_SOURCE_AGENT, "nli");
    }

    #[test]
    fn test_edge_validation_error_variants_exist() {
        // Each variant must construct without panic
        let _ = EdgeValidationError::UnknownType {
            edge_type: "x".to_string(),
        };
        let _ = EdgeValidationError::SelfReferential { id: 1 };
        let _ = EdgeValidationError::TargetNotFound { target_id: 1 };
        let _ = EdgeValidationError::TargetQuarantined { target_id: 1 };
    }

    #[test]
    fn test_edge_validation_error_display_unknown_type() {
        let e = EdgeValidationError::UnknownType {
            edge_type: "Bogus".to_string(),
        };
        assert!(e.to_string().contains("unknown edge type"));
        assert!(e.to_string().contains("Bogus"));
    }

    #[test]
    fn test_edge_validation_error_display_self_ref() {
        let e = EdgeValidationError::SelfReferential { id: 42 };
        assert!(e.to_string().contains("self-referential"));
        assert!(e.to_string().contains("42"));
    }

    #[test]
    fn test_edge_validation_error_display_not_found() {
        let e = EdgeValidationError::TargetNotFound { target_id: 99 };
        assert!(e.to_string().contains("99"));
        assert!(e.to_string().contains("does not exist"));
    }

    #[test]
    fn test_edge_validation_error_display_quarantined() {
        let e = EdgeValidationError::TargetQuarantined { target_id: 7 };
        assert!(e.to_string().contains("7"));
        assert!(e.to_string().contains("quarantined"));
    }

    #[test]
    fn test_edge_delete_error_display() {
        // Verify Display doesn't panic
        let e = EdgeDeleteError::StoreError(StoreError::EntryNotFound(5));
        let s = e.to_string();
        assert!(!s.is_empty());
    }

    #[test]
    fn test_edge_redirect_error_display_variants() {
        let e1 = EdgeRedirectError::TargetNotFound { target_id: 3 };
        assert!(e1.to_string().contains("3"));
        let e2 = EdgeRedirectError::TargetQuarantined { target_id: 8 };
        assert!(e2.to_string().contains("8"));
    }
}
