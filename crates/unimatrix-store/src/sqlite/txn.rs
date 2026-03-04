//! Transaction wrapper types for server API compatibility (ADR-001).
//! Provides API surface compatible with redb transaction/table types.
//!
//! The typed open_table methods are defined in compat_txn.rs, which
//! maps SqliteTableDef<K,V> to correctly-typed handle structs.

use std::sync::MutexGuard;
use rusqlite::Connection;
use crate::error::Result;

/// Read transaction wrapper. Wraps MutexGuard on the SQLite connection.
pub struct SqliteReadTransaction<'a> {
    pub(crate) guard: MutexGuard<'a, Connection>,
}

/// Write transaction wrapper for server compatibility (ADR-001).
///
/// Wraps a real SQLite transaction: `new()` executes `BEGIN IMMEDIATE`,
/// `commit()` executes `COMMIT`, and `Drop` executes `ROLLBACK` if not
/// committed. This ensures the server's `write_in_txn` pattern works
/// correctly (writes are only visible after explicit commit).
pub struct SqliteWriteTransaction<'a> {
    pub(crate) guard: MutexGuard<'a, Connection>,
    committed: bool,
}

impl<'a> SqliteWriteTransaction<'a> {
    /// Create a new write transaction wrapper, beginning a real SQL transaction.
    pub(crate) fn new(guard: MutexGuard<'a, Connection>) -> crate::error::Result<Self> {
        guard
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(crate::error::StoreError::Sqlite)?;
        Ok(Self { guard, committed: false })
    }

    /// Commit the transaction.
    pub fn commit(mut self) -> Result<()> {
        self.guard
            .execute_batch("COMMIT")
            .map_err(crate::error::StoreError::Sqlite)?;
        self.committed = true;
        Ok(())
    }
}

impl<'a> Drop for SqliteWriteTransaction<'a> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.guard.execute_batch("ROLLBACK");
        }
    }
}

// ---------------------------------------------------------------------------
// Table name -> column name mapping (used by compat_handles and Store methods)
// ---------------------------------------------------------------------------

/// Map table name to its primary key column name.
pub(crate) fn primary_key_column(table_name: &str) -> &'static str {
    match table_name {
        "entries" => "id",
        "topic_index" => "topic",
        "category_index" => "category",
        "tag_index" => "tag",
        "time_index" => "timestamp",
        "status_index" => "status",
        "vector_map" => "entry_id",
        "counters" => "name",
        "agent_registry" => "agent_id",
        "audit_log" => "event_id",
        "feature_entries" => "feature_id",
        "co_access" => "entry_id_a",
        "outcome_index" => "feature_cycle",
        "observation_metrics" => "feature_cycle",
        "signal_queue" => "signal_id",
        "sessions" => "session_id",
        "injection_log" => "log_id",
        _ => "id",
    }
}

/// Map table name to its data/value column name.
pub(crate) fn data_column(table_name: &str) -> &'static str {
    match table_name {
        "vector_map" => "hnsw_data_id",
        "counters" => "value",
        _ => "data",
    }
}
