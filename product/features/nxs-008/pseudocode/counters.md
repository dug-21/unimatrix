# Component: counters (Wave 0)

## File: `crates/unimatrix-store/src/counters.rs`

**Action**: CREATE (~60 lines)
**Risk**: Low (RISK-15)
**ADR**: ADR-002

## Purpose

Consolidate counter helpers from `write.rs` (private fns) and `tables.rs` (pub fns taking `&SqliteWriteTransaction`). New module provides 5 functions all taking `&Connection`, usable by both store-crate internal code and server-crate code via re-export.

## Pseudocode

```rust
//! Counter helpers for the COUNTERS table.
//!
//! All functions take &Connection directly (ADR-002).
//! Consolidated from write.rs and tables.rs.

use rusqlite::{Connection, OptionalExtension};
use crate::error::{Result, StoreError};

/// Read a counter value. Returns 0 if counter does not exist.
pub(crate) fn read_counter(conn: &Connection, name: &str) -> Result<u64> {
    let val: Option<i64> = conn
        .query_row(
            "SELECT value FROM counters WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .optional()
        .map_err(StoreError::Sqlite)?;
    Ok(val.unwrap_or(0) as u64)
}

/// Set a counter to a specific value.
pub(crate) fn set_counter(conn: &Connection, name: &str, value: u64) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)",
        rusqlite::params![name, value as i64],
    )
    .map_err(StoreError::Sqlite)?;
    Ok(())
}

/// Increment a counter by delta.
pub(crate) fn increment_counter(conn: &Connection, name: &str, delta: u64) -> Result<()> {
    let current = read_counter(conn, name)?;
    set_counter(conn, name, current + delta)
}

/// Decrement a counter by delta (saturating at 0).
pub(crate) fn decrement_counter(conn: &Connection, name: &str, delta: u64) -> Result<()> {
    let current = read_counter(conn, name)?;
    set_counter(conn, name, current.saturating_sub(delta))
}

/// Allocate the next entry ID. Reads current, increments, returns old value.
pub(crate) fn next_entry_id(conn: &Connection) -> Result<u64> {
    let current = read_counter(conn, "next_entry_id")?;
    // Ensure counter exists with at least 1
    let id = if current == 0 { 1 } else { current };
    set_counter(conn, "next_entry_id", id + 1)?;
    Ok(id)
}
```

## Re-exports

In `lib.rs`, add:
```rust
pub(crate) mod counters;

// Public re-exports for server crate
pub use counters::{read_counter as counter_read, set_counter as counter_set,
                   increment_counter as counter_increment,
                   decrement_counter as counter_decrement,
                   next_entry_id as counter_next_entry_id};
```

**Note**: The re-export names avoid collision with the existing `tables::next_entry_id` etc. During Wave 4, the tables.rs re-exports are removed and these become the sole exports. Alternatively, we can keep the same names and just change the source module. The server-crate imports from `unimatrix_store::{next_entry_id, increment_counter, decrement_counter}` will continue to work if we re-export with the same names.

**Revised approach** (simpler): Re-export with same names. Since tables.rs counter helpers take `&SqliteWriteTransaction` and new ones take `&Connection`, the signatures differ. Server code must be updated in Wave 1 to use `&*txn.guard` pattern. During Wave 0, both exist. During Wave 4, tables.rs is deleted.

## Changes to lib.rs (Wave 0)

```rust
mod counters;
// Keep existing tables re-exports until Wave 4
```

## Changes to write.rs (Wave 1)

Remove the 4 private counter functions (`read_counter`, `set_counter`, `increment_counter`, `decrement_counter`). Replace calls with `crate::counters::*`.
