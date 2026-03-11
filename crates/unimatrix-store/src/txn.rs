//! Transaction wrapper for atomic write operations (ADR-001, nxs-008).

use rusqlite::Connection;
use std::sync::MutexGuard;

/// Write transaction wrapper (ADR-001).
///
/// Wraps a real SQLite transaction: `new()` executes `BEGIN IMMEDIATE`,
/// `commit()` executes `COMMIT`, and `Drop` executes `ROLLBACK` if not
/// committed.
pub struct SqliteWriteTransaction<'a> {
    /// The underlying connection guard. Public for direct SQL access in downstream crates.
    pub guard: MutexGuard<'a, Connection>,
    committed: bool,
}

impl<'a> SqliteWriteTransaction<'a> {
    /// Create a new write transaction wrapper, beginning a real SQL transaction.
    pub(crate) fn new(guard: MutexGuard<'a, Connection>) -> crate::error::Result<Self> {
        guard
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(crate::error::StoreError::Sqlite)?;
        Ok(Self {
            guard,
            committed: false,
        })
    }

    /// Commit the transaction.
    pub fn commit(mut self) -> crate::error::Result<()> {
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
