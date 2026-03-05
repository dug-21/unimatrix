//! Table definitions and guard types for server database access.
//!
//! These types provide the server's database access API. They will be
//! replaced when the server migrates to the Store trait API (nxs-008).

use std::marker::PhantomData;

use crate::txn::SqliteWriteTransaction;
use crate::error::{Result, StoreError};

// ---------------------------------------------------------------------------
// Marker types for table key/value type parameters
// ---------------------------------------------------------------------------

pub struct Str;
pub struct Blob;
pub struct U64Val;
pub struct Unit;
pub struct StrU64Key;
pub struct U64U64Key;
pub struct U8U64Key;

// ---------------------------------------------------------------------------
// Table definition types
// ---------------------------------------------------------------------------

pub struct SqliteTableDef<K, V> {
    pub name: &'static str,
    _phantom: PhantomData<(K, V)>,
}

impl<K, V> SqliteTableDef<K, V> {
    pub const fn new(name: &'static str) -> Self {
        Self { name, _phantom: PhantomData }
    }
}

pub struct SqliteMultimapDef<K, V> {
    pub name: &'static str,
    _phantom: PhantomData<(K, V)>,
}

impl<K, V> SqliteMultimapDef<K, V> {
    pub const fn new(name: &'static str) -> Self {
        Self { name, _phantom: PhantomData }
    }
}

// ---------------------------------------------------------------------------
// Typed table constants
// ---------------------------------------------------------------------------

pub const ENTRIES: SqliteTableDef<U64Val, Blob> = SqliteTableDef::new("entries");
pub const TOPIC_INDEX: SqliteTableDef<StrU64Key, Unit> = SqliteTableDef::new("topic_index");
pub const CATEGORY_INDEX: SqliteTableDef<StrU64Key, Unit> = SqliteTableDef::new("category_index");
pub const TAG_INDEX: SqliteMultimapDef<Str, U64Val> = SqliteMultimapDef::new("tag_index");
pub const TIME_INDEX: SqliteTableDef<U64U64Key, Unit> = SqliteTableDef::new("time_index");
pub const STATUS_INDEX: SqliteTableDef<U8U64Key, Unit> = SqliteTableDef::new("status_index");
pub const VECTOR_MAP: SqliteTableDef<U64Val, U64Val> = SqliteTableDef::new("vector_map");
pub const COUNTERS: SqliteTableDef<Str, U64Val> = SqliteTableDef::new("counters");
pub const OUTCOME_INDEX: SqliteTableDef<StrU64Key, Unit> = SqliteTableDef::new("outcome_index");
pub const AUDIT_LOG: SqliteTableDef<U64Val, Blob> = SqliteTableDef::new("audit_log");
pub const AGENT_REGISTRY: SqliteTableDef<Str, Blob> = SqliteTableDef::new("agent_registry");
pub const FEATURE_ENTRIES: SqliteMultimapDef<Str, U64Val> = SqliteMultimapDef::new("feature_entries");
pub const CO_ACCESS: SqliteTableDef<U64U64Key, Blob> = SqliteTableDef::new("co_access");
pub const SIGNAL_QUEUE: SqliteTableDef<U64Val, Blob> = SqliteTableDef::new("signal_queue");
pub const SESSIONS: SqliteTableDef<Str, Blob> = SqliteTableDef::new("sessions");
pub const INJECTION_LOG: SqliteTableDef<U64Val, Blob> = SqliteTableDef::new("injection_log");

// ---------------------------------------------------------------------------
// Counter helpers
// ---------------------------------------------------------------------------

pub fn next_entry_id(txn: &SqliteWriteTransaction<'_>) -> Result<u64> {
    let conn = &*txn.guard;
    let current: i64 = conn
        .query_row(
            "SELECT value FROM counters WHERE name = 'next_entry_id'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(1);
    conn.execute(
        "INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', ?1)",
        rusqlite::params![current + 1],
    )
    .map_err(StoreError::Sqlite)?;
    Ok(current as u64)
}

pub fn increment_counter(
    txn: &SqliteWriteTransaction<'_>,
    key: &str,
    delta: u64,
) -> Result<()> {
    let conn = &*txn.guard;
    let current: i64 = conn
        .query_row(
            "SELECT value FROM counters WHERE name = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        )
        .unwrap_or(0);
    conn.execute(
        "INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)",
        rusqlite::params![key, current + delta as i64],
    )
    .map_err(StoreError::Sqlite)?;
    Ok(())
}

pub fn decrement_counter(
    txn: &SqliteWriteTransaction<'_>,
    key: &str,
    amount: u64,
) -> Result<()> {
    let conn = &*txn.guard;
    let current: i64 = conn
        .query_row(
            "SELECT value FROM counters WHERE name = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let new_val = (current as u64).saturating_sub(amount) as i64;
    conn.execute(
        "INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)",
        rusqlite::params![key, new_val],
    )
    .map_err(StoreError::Sqlite)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// AccessGuard-compatible wrappers
// ---------------------------------------------------------------------------

pub struct BlobGuard(pub Vec<u8>);
impl BlobGuard {
    pub fn value(&self) -> &[u8] { &self.0 }
}

pub struct U64Guard(pub u64);
impl U64Guard {
    pub fn value(&self) -> u64 { self.0 }
}

pub struct UnitGuard;

pub struct CompositeKeyGuard(pub String, pub u64);
impl CompositeKeyGuard {
    pub fn value(&self) -> (&str, u64) { (&self.0, self.1) }
}

pub struct U64KeyGuard(pub u64);
impl U64KeyGuard {
    pub fn value(&self) -> u64 { self.0 }
}

// ---------------------------------------------------------------------------
// Range result wrapper (provides .count() + IntoIterator for server compat)
// ---------------------------------------------------------------------------

/// Wraps a Vec of results so that both `.count()` and `for item in range`
/// work, matching the Range iterator API expected by callers.
pub struct RangeResult<T>(pub Vec<T>);

impl<T> RangeResult<T> {
    /// Return the number of items (matches Iterator::count() semantics).
    pub fn count(self) -> usize {
        self.0.len()
    }
}

impl<T> IntoIterator for RangeResult<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
