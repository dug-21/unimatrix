//! Typed open_table dispatch for server transaction compatibility.
//!
//! Maps SqliteTableDef<K,V> to the right typed handle via the TableSpec
//! trait. TEMPORARY: will be removed with the server Store API migration.

use rusqlite::Connection;

use crate::error::Result;
use super::compat::{
    Blob, Str, U64Val, Unit, StrU64Key, U64U64Key, U8U64Key,
    SqliteTableDef, SqliteMultimapDef,
};
use super::compat_handles::{
    TableU64Blob, TableStrU64, TableStrBlob,
    TableStrU64Comp, TableU64U64Comp, TableU8U64Comp,
    TableU64U64, MultimapStrU64,
};
use super::txn::{SqliteReadTransaction, SqliteWriteTransaction};

// ---------------------------------------------------------------------------
// TableSpec: maps (K, V) marker types to a concrete handle constructor
// ---------------------------------------------------------------------------

pub trait TableSpec {
    type Handle<'a>;
    fn name(&self) -> &'static str;
    fn make<'a>(conn: &'a Connection, name: &'static str) -> Self::Handle<'a>;
}

pub trait MultimapSpec {
    type Handle<'a>;
    fn name(&self) -> &'static str;
    fn make<'a>(conn: &'a Connection, name: &'static str) -> Self::Handle<'a>;
}

// -- Impls: one per (K, V) table type --

impl TableSpec for SqliteTableDef<U64Val, Blob> {
    type Handle<'a> = TableU64Blob<'a>;
    fn name(&self) -> &'static str { self.name }
    fn make<'a>(conn: &'a Connection, name: &'static str) -> TableU64Blob<'a> {
        TableU64Blob { conn, table_name: name }
    }
}

impl TableSpec for SqliteTableDef<Str, U64Val> {
    type Handle<'a> = TableStrU64<'a>;
    fn name(&self) -> &'static str { self.name }
    fn make<'a>(conn: &'a Connection, name: &'static str) -> TableStrU64<'a> {
        TableStrU64 { conn, table_name: name }
    }
}

impl TableSpec for SqliteTableDef<Str, Blob> {
    type Handle<'a> = TableStrBlob<'a>;
    fn name(&self) -> &'static str { self.name }
    fn make<'a>(conn: &'a Connection, name: &'static str) -> TableStrBlob<'a> {
        TableStrBlob { conn, table_name: name }
    }
}

impl TableSpec for SqliteTableDef<StrU64Key, Unit> {
    type Handle<'a> = TableStrU64Comp<'a>;
    fn name(&self) -> &'static str { self.name }
    fn make<'a>(conn: &'a Connection, name: &'static str) -> TableStrU64Comp<'a> {
        TableStrU64Comp { conn, table_name: name }
    }
}

impl TableSpec for SqliteTableDef<U64U64Key, Unit> {
    type Handle<'a> = TableU64U64Comp<'a>;
    fn name(&self) -> &'static str { self.name }
    fn make<'a>(conn: &'a Connection, name: &'static str) -> TableU64U64Comp<'a> {
        TableU64U64Comp { conn, table_name: name }
    }
}

impl TableSpec for SqliteTableDef<U8U64Key, Unit> {
    type Handle<'a> = TableU8U64Comp<'a>;
    fn name(&self) -> &'static str { self.name }
    fn make<'a>(conn: &'a Connection, name: &'static str) -> TableU8U64Comp<'a> {
        TableU8U64Comp { conn, table_name: name }
    }
}

impl TableSpec for SqliteTableDef<U64Val, U64Val> {
    type Handle<'a> = TableU64U64<'a>;
    fn name(&self) -> &'static str { self.name }
    fn make<'a>(conn: &'a Connection, name: &'static str) -> TableU64U64<'a> {
        TableU64U64 { conn, table_name: name }
    }
}

impl TableSpec for SqliteTableDef<U64U64Key, Blob> {
    type Handle<'a> = TableU64Blob<'a>;
    fn name(&self) -> &'static str { self.name }
    fn make<'a>(conn: &'a Connection, name: &'static str) -> TableU64Blob<'a> {
        TableU64Blob { conn, table_name: name }
    }
}

impl MultimapSpec for SqliteMultimapDef<Str, U64Val> {
    type Handle<'a> = MultimapStrU64<'a>;
    fn name(&self) -> &'static str { self.name }
    fn make<'a>(conn: &'a Connection, name: &'static str) -> MultimapStrU64<'a> {
        MultimapStrU64 { conn, table_name: name }
    }
}

// ---------------------------------------------------------------------------
// open_table / open_multimap_table on transactions
// ---------------------------------------------------------------------------

impl<'txn> SqliteReadTransaction<'txn> {
    pub fn open_table<T: TableSpec>(&self, def: T) -> Result<T::Handle<'_>> {
        Ok(T::make(&self.guard, def.name()))
    }

    pub fn open_multimap_table<T: MultimapSpec>(&self, def: T) -> Result<T::Handle<'_>> {
        Ok(T::make(&self.guard, def.name()))
    }
}

impl<'txn> SqliteWriteTransaction<'txn> {
    pub fn open_table<T: TableSpec>(&self, def: T) -> Result<T::Handle<'_>> {
        Ok(T::make(&self.guard, def.name()))
    }

    pub fn open_multimap_table<T: MultimapSpec>(&self, def: T) -> Result<T::Handle<'_>> {
        Ok(T::make(&self.guard, def.name()))
    }
}
