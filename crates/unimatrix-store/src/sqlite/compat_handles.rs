//! Typed table handle implementations for server compatibility.
//!
//! Each handle type maps to a specific (key, value) combination from
//! the redb table schema. Methods match redb's ReadableTable trait API.
//! TEMPORARY: will be removed when the server migrates to the Store API.

use rusqlite::{Connection, OptionalExtension};

use crate::error::{Result, StoreError};
use super::compat::{BlobGuard, U64Guard, UnitGuard, CompositeKeyGuard, U64KeyGuard, RangeResult};
use super::txn::{primary_key_column, data_column};

/// Iterator over multimap values, matching redb MultimapRange.
pub struct MultimapIter(std::vec::IntoIter<std::result::Result<U64Guard, StoreError>>);

impl Iterator for MultimapIter {
    type Item = std::result::Result<U64Guard, StoreError>;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

// ---------------------------------------------------------------------------
// TableU64Blob: u64 key -> blob value (ENTRIES, AUDIT_LOG, SIGNAL_QUEUE)
// ---------------------------------------------------------------------------

pub struct TableU64Blob<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: &'static str,
}

impl<'a> TableU64Blob<'a> {
    pub fn get(&self, key: u64) -> Result<Option<BlobGuard>> {
        let kc = primary_key_column(self.table_name);
        let dc = data_column(self.table_name);
        let sql = format!("SELECT {} FROM {} WHERE {} = ?1", dc, self.table_name, kc);
        self.conn
            .query_row(&sql, rusqlite::params![key as i64], |r| r.get::<_, Vec<u8>>(0))
            .optional()
            .map(|o| o.map(BlobGuard))
            .map_err(StoreError::Sqlite)
    }

    pub fn insert(&self, key: u64, value: &[u8]) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let dc = data_column(self.table_name);
        let sql = format!("INSERT OR REPLACE INTO {} ({}, {}) VALUES (?1, ?2)", self.table_name, kc, dc);
        self.conn.execute(&sql, rusqlite::params![key as i64, value]).map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn remove(&self, key: u64) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let sql = format!("DELETE FROM {} WHERE {} = ?1", self.table_name, kc);
        self.conn.execute(&sql, rusqlite::params![key as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn iter(
        &self,
    ) -> Result<Vec<std::result::Result<(U64KeyGuard, BlobGuard), StoreError>>> {
        let kc = primary_key_column(self.table_name);
        let dc = data_column(self.table_name);
        let sql = format!("SELECT {}, {} FROM {} ORDER BY {}", kc, dc, self.table_name, kc);
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, Vec<u8>>(1)?)))
            .map_err(StoreError::Sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map(|(k, v)| (U64KeyGuard(k), BlobGuard(v))).map_err(StoreError::Sqlite));
        }
        Ok(out)
    }

    pub fn len(&self) -> Result<u64> {
        let sql = format!("SELECT COUNT(*) FROM {}", self.table_name);
        let c: i64 = self.conn.query_row(&sql, [], |r| r.get(0)).map_err(StoreError::Sqlite)?;
        Ok(c as u64)
    }

    pub fn is_empty(&self) -> Result<bool> {
        self.len().map(|n| n == 0)
    }
}

// ---------------------------------------------------------------------------
// TableStrU64: str key -> u64 value (COUNTERS)
// ---------------------------------------------------------------------------

pub struct TableStrU64<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: &'static str,
}

impl<'a> TableStrU64<'a> {
    pub fn get(&self, key: &str) -> Result<Option<U64Guard>> {
        let kc = primary_key_column(self.table_name);
        let dc = data_column(self.table_name);
        let sql = format!("SELECT {} FROM {} WHERE {} = ?1", dc, self.table_name, kc);
        self.conn
            .query_row(&sql, rusqlite::params![key], |r| Ok(r.get::<_, i64>(0)? as u64))
            .optional()
            .map(|o| o.map(U64Guard))
            .map_err(StoreError::Sqlite)
    }

    pub fn insert(&self, key: &str, value: u64) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let dc = data_column(self.table_name);
        let sql = format!("INSERT OR REPLACE INTO {} ({}, {}) VALUES (?1, ?2)", self.table_name, kc, dc);
        self.conn.execute(&sql, rusqlite::params![key, value as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TableStrBlob: str key -> blob value (AGENT_REGISTRY, SESSIONS)
// ---------------------------------------------------------------------------

pub struct TableStrBlob<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: &'static str,
}

impl<'a> TableStrBlob<'a> {
    pub fn get(&self, key: &str) -> Result<Option<BlobGuard>> {
        let kc = primary_key_column(self.table_name);
        let dc = data_column(self.table_name);
        let sql = format!("SELECT {} FROM {} WHERE {} = ?1", dc, self.table_name, kc);
        self.conn
            .query_row(&sql, rusqlite::params![key], |r| r.get::<_, Vec<u8>>(0))
            .optional()
            .map(|o| o.map(BlobGuard))
            .map_err(StoreError::Sqlite)
    }

    pub fn insert(&self, key: &str, value: &[u8]) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let dc = data_column(self.table_name);
        let sql = format!("INSERT OR REPLACE INTO {} ({}, {}) VALUES (?1, ?2)", self.table_name, kc, dc);
        self.conn.execute(&sql, rusqlite::params![key, value]).map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn iter(&self) -> Result<Vec<std::result::Result<(String, BlobGuard), StoreError>>> {
        let kc = primary_key_column(self.table_name);
        let dc = data_column(self.table_name);
        let sql = format!("SELECT {}, {} FROM {} ORDER BY {}", kc, dc, self.table_name, kc);
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))
            .map_err(StoreError::Sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map(|(k, v)| (k, BlobGuard(v))).map_err(StoreError::Sqlite));
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// TableStrU64Comp: (str, u64) key -> unit (TOPIC/CATEGORY/OUTCOME_INDEX)
// ---------------------------------------------------------------------------

pub struct TableStrU64Comp<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: &'static str,
}

impl<'a> TableStrU64Comp<'a> {
    pub fn insert(&self, key: (&str, u64), _value: ()) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "INSERT OR IGNORE INTO {} ({}, entry_id) VALUES (?1, ?2)",
            self.table_name, kc
        );
        self.conn.execute(&sql, rusqlite::params![key.0, key.1 as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn remove(&self, key: (&str, u64)) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "DELETE FROM {} WHERE {} = ?1 AND entry_id = ?2",
            self.table_name, kc
        );
        self.conn.execute(&sql, rusqlite::params![key.0, key.1 as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn iter(
        &self,
    ) -> Result<Vec<std::result::Result<(CompositeKeyGuard, UnitGuard), StoreError>>> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "SELECT {}, entry_id FROM {} ORDER BY {}, entry_id",
            kc, self.table_name, kc
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))
            .map_err(StoreError::Sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(
                row.map(|(k, v)| (CompositeKeyGuard(k, v), UnitGuard))
                    .map_err(StoreError::Sqlite),
            );
        }
        Ok(out)
    }

    #[allow(clippy::extra_unused_type_parameters)]
    pub fn range<T>(
        &self,
        range: std::ops::RangeInclusive<(&str, u64)>,
    ) -> Result<RangeResult<std::result::Result<(CompositeKeyGuard, UnitGuard), StoreError>>> {
        let prefix = range.start().0;
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "SELECT {}, entry_id FROM {} WHERE {} = ?1 ORDER BY entry_id",
            kc, self.table_name, kc
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![prefix], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
            })
            .map_err(StoreError::Sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(
                row.map(|(k, v)| (CompositeKeyGuard(k, v), UnitGuard))
                    .map_err(StoreError::Sqlite),
            );
        }
        Ok(RangeResult(out))
    }
}

// ---------------------------------------------------------------------------
// TableU64U64Comp: (u64, u64) key -> unit (TIME_INDEX)
// ---------------------------------------------------------------------------

pub struct TableU64U64Comp<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: &'static str,
}

impl<'a> TableU64U64Comp<'a> {
    pub fn insert(&self, key: (u64, u64), _value: ()) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "INSERT OR IGNORE INTO {} ({}, entry_id) VALUES (?1, ?2)",
            self.table_name, kc
        );
        self.conn.execute(&sql, rusqlite::params![key.0 as i64, key.1 as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn remove(&self, key: (u64, u64)) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "DELETE FROM {} WHERE {} = ?1 AND entry_id = ?2",
            self.table_name, kc
        );
        self.conn.execute(&sql, rusqlite::params![key.0 as i64, key.1 as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TableU8U64Comp: (u8, u64) key -> unit (STATUS_INDEX)
// ---------------------------------------------------------------------------

pub struct TableU8U64Comp<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: &'static str,
}

impl<'a> TableU8U64Comp<'a> {
    pub fn get(&self, key: (u8, u64)) -> Result<Option<UnitGuard>> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "SELECT 1 FROM {} WHERE {} = ?1 AND entry_id = ?2",
            self.table_name, kc
        );
        self.conn
            .query_row(&sql, rusqlite::params![key.0, key.1 as i64], |_| Ok(()))
            .optional()
            .map(|o| o.map(|_| UnitGuard))
            .map_err(StoreError::Sqlite)
    }

    pub fn insert(&self, key: (u8, u64), _value: ()) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "INSERT OR IGNORE INTO {} ({}, entry_id) VALUES (?1, ?2)",
            self.table_name, kc
        );
        self.conn.execute(&sql, rusqlite::params![key.0, key.1 as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn remove(&self, key: (u8, u64)) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "DELETE FROM {} WHERE {} = ?1 AND entry_id = ?2",
            self.table_name, kc
        );
        self.conn.execute(&sql, rusqlite::params![key.0, key.1 as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }

    #[allow(clippy::extra_unused_type_parameters)]
    pub fn range<T>(
        &self,
        range: std::ops::RangeInclusive<(u8, u64)>,
    ) -> Result<RangeResult<std::result::Result<(CompositeKeyGuard, UnitGuard), StoreError>>> {
        let byte = range.start().0;
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "SELECT {}, entry_id FROM {} WHERE {} = ?1 ORDER BY entry_id",
            kc, self.table_name, kc
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![byte], |r| {
                let s = r.get::<_, u8>(0)?;
                let eid = r.get::<_, i64>(1)? as u64;
                Ok((s.to_string(), eid))
            })
            .map_err(StoreError::Sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(
                row.map(|(k, v)| (CompositeKeyGuard(k, v), UnitGuard))
                    .map_err(StoreError::Sqlite),
            );
        }
        Ok(RangeResult(out))
    }
}

// ---------------------------------------------------------------------------
// TableU64U64: u64 key -> u64 value (VECTOR_MAP)
// ---------------------------------------------------------------------------

pub struct TableU64U64<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: &'static str,
}

impl<'a> TableU64U64<'a> {
    pub fn insert(&self, key: u64, value: u64) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let dc = data_column(self.table_name);
        let sql = format!(
            "INSERT OR REPLACE INTO {} ({}, {}) VALUES (?1, ?2)",
            self.table_name, kc, dc
        );
        self.conn.execute(&sql, rusqlite::params![key as i64, value as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn remove(&self, key: u64) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let sql = format!("DELETE FROM {} WHERE {} = ?1", self.table_name, kc);
        self.conn.execute(&sql, rusqlite::params![key as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MultimapStrU64: str key -> set of u64 (TAG_INDEX, FEATURE_ENTRIES)
// ---------------------------------------------------------------------------

pub struct MultimapStrU64<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) table_name: &'static str,
}

impl<'a> MultimapStrU64<'a> {
    /// Get all values for a key, returning an iterator of Result<U64Guard>.
    /// Matches the redb ReadableMultimapTable::get() API shape.
    pub fn get(&self, key: &str) -> Result<MultimapIter> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "SELECT entry_id FROM {} WHERE {} = ?1 ORDER BY entry_id",
            self.table_name, kc
        );
        let mut stmt = self.conn.prepare(&sql).map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![key], |r| Ok(r.get::<_, i64>(0)? as u64))
            .map_err(StoreError::Sqlite)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map(U64Guard).map_err(StoreError::Sqlite));
        }
        Ok(MultimapIter(items.into_iter()))
    }

    pub fn insert(&self, key: &str, value: u64) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "INSERT OR IGNORE INTO {} ({}, entry_id) VALUES (?1, ?2)",
            self.table_name, kc
        );
        self.conn.execute(&sql, rusqlite::params![key, value as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn remove(&self, key: &str, value: u64) -> Result<()> {
        let kc = primary_key_column(self.table_name);
        let sql = format!(
            "DELETE FROM {} WHERE {} = ?1 AND entry_id = ?2",
            self.table_name, kc
        );
        self.conn.execute(&sql, rusqlite::params![key, value as i64]).map_err(StoreError::Sqlite)?;
        Ok(())
    }
}
