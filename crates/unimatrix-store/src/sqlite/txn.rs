//! Transaction wrapper types for server API compatibility (ADR-001).
//! Provides API surface compatible with redb transaction/table types.

use std::sync::MutexGuard;
use rusqlite::{Connection, OptionalExtension};
use crate::error::{Result, StoreError};

/// Read transaction wrapper. Wraps MutexGuard on the SQLite connection.
pub struct SqliteReadTransaction<'a> {
    pub(crate) guard: MutexGuard<'a, Connection>,
}

impl<'a> SqliteReadTransaction<'a> {
    /// Open a table for reading. Returns a handle that supports get/iter.
    pub fn open_table<K, V>(
        &self,
        table_name: &str,
    ) -> Result<SqliteTableHandle<'_>> {
        Ok(SqliteTableHandle {
            conn: &self.guard,
            table_name: table_name.to_string(),
        })
    }

    /// Open a multimap table for reading.
    pub fn open_multimap_table<K, V>(
        &self,
        table_name: &str,
    ) -> Result<SqliteMultimapTableHandle<'_>> {
        Ok(SqliteMultimapTableHandle {
            conn: &self.guard,
            table_name: table_name.to_string(),
        })
    }
}

/// Write transaction wrapper for server compatibility (ADR-001).
pub struct SqliteWriteTransaction<'a> {
    pub(crate) guard: MutexGuard<'a, Connection>,
    committed: bool,
}

impl<'a> SqliteWriteTransaction<'a> {
    /// Create a new write transaction wrapper.
    pub(crate) fn new(guard: MutexGuard<'a, Connection>) -> Self {
        Self { guard, committed: false }
    }

    /// Open a table for writing. Returns a mutable handle.
    pub fn open_table<K, V>(
        &self,
        table_name: &str,
    ) -> Result<SqliteMutableTableHandle<'_>> {
        Ok(SqliteMutableTableHandle {
            conn: &self.guard,
            table_name: table_name.to_string(),
        })
    }

    /// Open a multimap table for writing.
    pub fn open_multimap_table<K, V>(
        &self,
        table_name: &str,
    ) -> Result<SqliteMutableMultimapTableHandle<'_>> {
        Ok(SqliteMutableMultimapTableHandle {
            conn: &self.guard,
            table_name: table_name.to_string(),
        })
    }

    /// Commit the transaction.
    pub fn commit(mut self) -> Result<()> {
        self.committed = true;
        Ok(())
    }
}

impl<'a> Drop for SqliteWriteTransaction<'a> {
    fn drop(&mut self) {
        // No-op: SQLite transactions are managed by the individual Store methods
        // using explicit BEGIN/COMMIT. The write transaction wrapper is for
        // API compatibility with the server's usage pattern.
        let _ = self.committed;
    }
}

/// Read-only table handle for server compatibility.
pub struct SqliteTableHandle<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: String,
}

impl<'a> SqliteTableHandle<'a> {
    /// Get a value by string key.
    pub fn get_by_str(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let col = primary_key_column(&self.table_name);
        let data_col = data_column(&self.table_name);
        let sql = format!(
            "SELECT {} FROM {} WHERE {} = ?1",
            data_col, self.table_name, col
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let result = stmt
            .query_row(rusqlite::params![key], |row| row.get::<_, Vec<u8>>(0))
            .optional()
            .map_err(StoreError::Sqlite)?;
        Ok(result)
    }

    /// Get a value by u64 key.
    pub fn get_by_u64(&self, key: u64) -> Result<Option<Vec<u8>>> {
        let col = primary_key_column(&self.table_name);
        let data_col = data_column(&self.table_name);
        let sql = format!(
            "SELECT {} FROM {} WHERE {} = ?1",
            data_col, self.table_name, col
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let result = stmt
            .query_row(rusqlite::params![key as i64], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .optional()
            .map_err(StoreError::Sqlite)?;
        Ok(result)
    }

    /// Iterate all rows as (key_str, value_bytes).
    pub fn iter_str_blob(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let col = primary_key_column(&self.table_name);
        let data_col = data_column(&self.table_name);
        let sql = format!(
            "SELECT {}, {} FROM {} ORDER BY {}",
            col, data_col, self.table_name, col
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }

    /// Iterate all rows as (u64_key, value_bytes).
    pub fn iter_u64_blob(&self) -> Result<Vec<(u64, Vec<u8>)>> {
        let col = primary_key_column(&self.table_name);
        let data_col = data_column(&self.table_name);
        let sql = format!(
            "SELECT {}, {} FROM {} ORDER BY {}",
            col, data_col, self.table_name, col
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }

    /// Count rows in the table.
    pub fn len(&self) -> Result<u64> {
        let sql = format!("SELECT COUNT(*) FROM {}", self.table_name);
        let count: i64 = self
            .conn
            .query_row(&sql, [], |row| row.get(0))
            .map_err(StoreError::Sqlite)?;
        Ok(count as u64)
    }

    /// Range scan for composite key tables like OUTCOME_INDEX: (str, u64).
    pub fn range_str_u64(&self, prefix: &str) -> Result<Vec<(String, u64)>> {
        let sql = format!(
            "SELECT {}, entry_id FROM {} WHERE {} = ?1 ORDER BY entry_id",
            primary_key_column(&self.table_name),
            self.table_name,
            primary_key_column(&self.table_name),
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![prefix], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }
}

/// Mutable table handle for server compatibility.
pub struct SqliteMutableTableHandle<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: String,
}

impl<'a> SqliteMutableTableHandle<'a> {
    /// Insert or replace a row with string key.
    pub fn insert_str(&self, key: &str, value: &[u8]) -> Result<()> {
        let col = primary_key_column(&self.table_name);
        let data_col = data_column(&self.table_name);
        let sql = format!(
            "INSERT OR REPLACE INTO {} ({}, {}) VALUES (?1, ?2)",
            self.table_name, col, data_col
        );
        self.conn
            .execute(&sql, rusqlite::params![key, value])
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Insert or replace a row with u64 key.
    pub fn insert_u64(&self, key: u64, value: &[u8]) -> Result<()> {
        let col = primary_key_column(&self.table_name);
        let data_col = data_column(&self.table_name);
        let sql = format!(
            "INSERT OR REPLACE INTO {} ({}, {}) VALUES (?1, ?2)",
            self.table_name, col, data_col
        );
        self.conn
            .execute(&sql, rusqlite::params![key as i64, value])
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Insert a composite key row for OUTCOME_INDEX: (str, u64) -> ().
    pub fn insert_str_u64(&self, key_str: &str, key_u64: u64) -> Result<()> {
        let sql = format!(
            "INSERT OR IGNORE INTO {} ({}, entry_id) VALUES (?1, ?2)",
            self.table_name,
            primary_key_column(&self.table_name),
        );
        self.conn
            .execute(&sql, rusqlite::params![key_str, key_u64 as i64])
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Get a value by string key.
    pub fn get_by_str(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let col = primary_key_column(&self.table_name);
        let data_col = data_column(&self.table_name);
        let sql = format!(
            "SELECT {} FROM {} WHERE {} = ?1",
            data_col, self.table_name, col
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let result = stmt
            .query_row(rusqlite::params![key], |row| row.get::<_, Vec<u8>>(0))
            .optional()
            .map_err(StoreError::Sqlite)?;
        Ok(result)
    }

    /// Get a u64 value by string key (for COUNTERS).
    pub fn get_counter(&self, key: &str) -> Result<Option<u64>> {
        let sql = "SELECT value FROM counters WHERE name = ?1";
        let mut stmt = self.conn.prepare(sql).map_err(StoreError::Sqlite)?;
        let result = stmt
            .query_row(rusqlite::params![key], |row| {
                Ok(row.get::<_, i64>(0)? as u64)
            })
            .optional()
            .map_err(StoreError::Sqlite)?;
        Ok(result)
    }

    /// Set a counter value.
    pub fn set_counter(&self, key: &str, value: u64) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)",
                rusqlite::params![key, value as i64],
            )
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Remove a row by string key.
    pub fn remove_str(&self, key: &str) -> Result<()> {
        let col = primary_key_column(&self.table_name);
        let sql = format!("DELETE FROM {} WHERE {} = ?1", self.table_name, col);
        self.conn
            .execute(&sql, rusqlite::params![key])
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Remove a row by u64 key.
    pub fn remove_u64(&self, key: u64) -> Result<()> {
        let col = primary_key_column(&self.table_name);
        let sql = format!("DELETE FROM {} WHERE {} = ?1", self.table_name, col);
        self.conn
            .execute(&sql, rusqlite::params![key as i64])
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Iterate all rows as (u64_key, value_bytes).
    pub fn iter_u64_blob(&self) -> Result<Vec<(u64, Vec<u8>)>> {
        let col = primary_key_column(&self.table_name);
        let data_col = data_column(&self.table_name);
        let sql = format!(
            "SELECT {}, {} FROM {} ORDER BY {}",
            col, data_col, self.table_name, col
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }

    /// Iterate all rows as (str_key, value_bytes).
    pub fn iter_str_blob(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let col = primary_key_column(&self.table_name);
        let data_col = data_column(&self.table_name);
        let sql = format!(
            "SELECT {}, {} FROM {} ORDER BY {}",
            col, data_col, self.table_name, col
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }

    /// Count rows.
    pub fn len(&self) -> Result<u64> {
        let sql = format!("SELECT COUNT(*) FROM {}", self.table_name);
        let count: i64 = self
            .conn
            .query_row(&sql, [], |row| row.get(0))
            .map_err(StoreError::Sqlite)?;
        Ok(count as u64)
    }

    /// Range scan for composite key tables: (str_prefix, u64_range).
    pub fn range_str_u64(&self, prefix: &str) -> Result<Vec<(String, u64)>> {
        let sql = format!(
            "SELECT {}, entry_id FROM {} WHERE {} = ?1 ORDER BY entry_id",
            primary_key_column(&self.table_name),
            self.table_name,
            primary_key_column(&self.table_name),
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![prefix], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }
}

/// Multimap table handle (read-only) for server compatibility.
pub struct SqliteMultimapTableHandle<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: String,
}

impl<'a> SqliteMultimapTableHandle<'a> {
    /// Get all values for a string key (e.g., TAG_INDEX, FEATURE_ENTRIES).
    pub fn get_values(&self, key: &str) -> Result<Vec<u64>> {
        let col = primary_key_column(&self.table_name);
        let sql = format!(
            "SELECT entry_id FROM {} WHERE {} = ?1 ORDER BY entry_id",
            self.table_name, col
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![key], |row| {
                Ok(row.get::<_, i64>(0)? as u64)
            })
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }
}

/// Mutable multimap table handle for server compatibility.
pub struct SqliteMutableMultimapTableHandle<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: String,
}

impl<'a> SqliteMutableMultimapTableHandle<'a> {
    /// Insert a (string_key, u64_value) pair.
    pub fn insert(&self, key: &str, value: u64) -> Result<()> {
        let col = primary_key_column(&self.table_name);
        let sql = format!(
            "INSERT OR IGNORE INTO {} ({}, entry_id) VALUES (?1, ?2)",
            self.table_name, col
        );
        self.conn
            .execute(&sql, rusqlite::params![key, value as i64])
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Remove a specific (string_key, u64_value) pair.
    pub fn remove(&self, key: &str, value: u64) -> Result<()> {
        let col = primary_key_column(&self.table_name);
        let sql = format!(
            "DELETE FROM {} WHERE {} = ?1 AND entry_id = ?2",
            self.table_name, col
        );
        self.conn
            .execute(&sql, rusqlite::params![key, value as i64])
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Get all values for a string key.
    pub fn get_values(&self, key: &str) -> Result<Vec<u64>> {
        let col = primary_key_column(&self.table_name);
        let sql = format!(
            "SELECT entry_id FROM {} WHERE {} = ?1 ORDER BY entry_id",
            self.table_name, col
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![key], |row| {
                Ok(row.get::<_, i64>(0)? as u64)
            })
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }
}

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
