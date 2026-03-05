//! Data migration module for redb-to-SQLite cutover (nxs-006).
//!
//! Provides export (redb -> JSON-lines) and import (JSON-lines -> SQLite)
//! functionality. The two paths are mutually exclusive at compile time
//! via the `backend-sqlite` feature flag.

pub mod format;

#[cfg(not(feature = "backend-sqlite"))]
pub mod export;

#[cfg(feature = "backend-sqlite")]
pub mod import;

use format::{KeyType, ValueType};

/// Classification of each table's key/value schema for migration.
///
/// Used by both export and import to dispatch correct serialization
/// and deserialization logic per table.
#[derive(Debug, Clone, Copy)]
pub(crate) enum TableDescriptor {
    U64Blob { name: &'static str },
    StrBlob { name: &'static str },
    StrU64 { name: &'static str },
    StrU64Unit { name: &'static str },
    U64U64Unit { name: &'static str },
    U8U64Unit { name: &'static str },
    U64U64 { name: &'static str },
    U64U64Blob { name: &'static str },
    MultimapStrU64 { name: &'static str },
}

#[allow(dead_code)]
impl TableDescriptor {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::U64Blob { name }
            | Self::StrBlob { name }
            | Self::StrU64 { name }
            | Self::StrU64Unit { name }
            | Self::U64U64Unit { name }
            | Self::U8U64Unit { name }
            | Self::U64U64 { name }
            | Self::U64U64Blob { name }
            | Self::MultimapStrU64 { name } => name,
        }
    }

    pub(crate) fn key_type(&self) -> KeyType {
        match self {
            Self::U64Blob { .. } => KeyType::U64,
            Self::StrBlob { .. } => KeyType::Str,
            Self::StrU64 { .. } => KeyType::Str,
            Self::StrU64Unit { .. } => KeyType::StrU64,
            Self::U64U64Unit { .. } => KeyType::U64U64,
            Self::U8U64Unit { .. } => KeyType::U8U64,
            Self::U64U64 { .. } => KeyType::U64,
            Self::U64U64Blob { .. } => KeyType::U64U64,
            Self::MultimapStrU64 { .. } => KeyType::Str,
        }
    }

    pub(crate) fn value_type(&self) -> ValueType {
        match self {
            Self::U64Blob { .. } => ValueType::Blob,
            Self::StrBlob { .. } => ValueType::Blob,
            Self::StrU64 { .. } => ValueType::U64,
            Self::StrU64Unit { .. } => ValueType::Unit,
            Self::U64U64Unit { .. } => ValueType::Unit,
            Self::U8U64Unit { .. } => ValueType::Unit,
            Self::U64U64 { .. } => ValueType::U64,
            Self::U64U64Blob { .. } => ValueType::Blob,
            Self::MultimapStrU64 { .. } => ValueType::U64,
        }
    }

    pub(crate) fn is_multimap(&self) -> bool {
        matches!(self, Self::MultimapStrU64 { .. })
    }
}

/// All 17 tables in deterministic order, matching schema.rs definitions.
pub(crate) const ALL_TABLES: &[TableDescriptor] = &[
    TableDescriptor::U64Blob { name: "entries" },
    TableDescriptor::StrU64Unit { name: "topic_index" },
    TableDescriptor::StrU64Unit { name: "category_index" },
    TableDescriptor::MultimapStrU64 { name: "tag_index" },
    TableDescriptor::U64U64Unit { name: "time_index" },
    TableDescriptor::U8U64Unit { name: "status_index" },
    TableDescriptor::U64U64 { name: "vector_map" },
    TableDescriptor::StrU64 { name: "counters" },
    TableDescriptor::StrBlob { name: "agent_registry" },
    TableDescriptor::U64Blob { name: "audit_log" },
    TableDescriptor::MultimapStrU64 { name: "feature_entries" },
    TableDescriptor::U64U64Blob { name: "co_access" },
    TableDescriptor::StrU64Unit { name: "outcome_index" },
    TableDescriptor::StrBlob { name: "observation_metrics" },
    TableDescriptor::U64Blob { name: "signal_queue" },
    TableDescriptor::StrBlob { name: "sessions" },
    TableDescriptor::U64Blob { name: "injection_log" },
];

/// Summary of a migration operation (export or import).
#[derive(Debug)]
pub struct MigrationSummary {
    /// Per-table name and row count.
    pub tables: Vec<(String, u64)>,
}

impl MigrationSummary {
    pub(crate) fn new() -> Self {
        Self { tables: Vec::new() }
    }

    pub(crate) fn add(&mut self, name: &str, count: u64) {
        self.tables.push((name.to_string(), count));
    }

    /// Print a summary to stderr.
    pub fn print_to_stderr(&self) {
        let total: u64 = self.tables.iter().map(|(_, c)| c).sum();
        for (name, count) in &self.tables {
            eprintln!("  {name}: {count} rows");
        }
        eprintln!("  total: {total} rows across {} tables", self.tables.len());
    }
}

/// Error type for migration operations.
#[derive(Debug)]
pub enum MigrateError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Base64Decode(String),
    Store(crate::error::StoreError),
    RowCountMismatch {
        table: String,
        expected: u64,
        actual: u64,
    },
    Validation(String),
    #[cfg(not(feature = "backend-sqlite"))]
    Redb(redb::Error),
}

impl std::fmt::Display for MigrateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::Base64Decode(msg) => write!(f, "base64 decode error: {msg}"),
            Self::Store(e) => write!(f, "store error: {e}"),
            Self::RowCountMismatch {
                table,
                expected,
                actual,
            } => write!(
                f,
                "row count mismatch for {table}: expected {expected}, got {actual}"
            ),
            Self::Validation(msg) => write!(f, "validation error: {msg}"),
            #[cfg(not(feature = "backend-sqlite"))]
            Self::Redb(e) => write!(f, "redb error: {e}"),
        }
    }
}

impl std::error::Error for MigrateError {}

impl From<std::io::Error> for MigrateError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for MigrateError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<crate::error::StoreError> for MigrateError {
    fn from(e: crate::error::StoreError) -> Self {
        Self::Store(e)
    }
}

#[cfg(not(feature = "backend-sqlite"))]
impl From<redb::DatabaseError> for MigrateError {
    fn from(e: redb::DatabaseError) -> Self {
        Self::Redb(redb::Error::from(e))
    }
}

#[cfg(not(feature = "backend-sqlite"))]
impl From<redb::TransactionError> for MigrateError {
    fn from(e: redb::TransactionError) -> Self {
        Self::Redb(redb::Error::from(e))
    }
}

#[cfg(not(feature = "backend-sqlite"))]
impl From<redb::TableError> for MigrateError {
    fn from(e: redb::TableError) -> Self {
        Self::Redb(redb::Error::from(e))
    }
}

#[cfg(not(feature = "backend-sqlite"))]
impl From<redb::StorageError> for MigrateError {
    fn from(e: redb::StorageError) -> Self {
        Self::Redb(redb::Error::from(e))
    }
}
