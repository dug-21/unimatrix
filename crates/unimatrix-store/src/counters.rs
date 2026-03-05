//! Counter helpers for the COUNTERS table.
//!
//! All functions take &Connection directly (ADR-002).
//! Consolidated from write.rs and tables.rs.

use rusqlite::{Connection, OptionalExtension};

use crate::error::{Result, StoreError};

/// Read a counter value. Returns 0 if counter does not exist.
pub fn read_counter(conn: &Connection, name: &str) -> Result<u64> {
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
pub fn set_counter(conn: &Connection, name: &str, value: u64) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)",
        rusqlite::params![name, value as i64],
    )
    .map_err(StoreError::Sqlite)?;
    Ok(())
}

/// Increment a counter by delta.
pub fn increment_counter(conn: &Connection, name: &str, delta: u64) -> Result<()> {
    let current = read_counter(conn, name)?;
    set_counter(conn, name, current + delta)
}

/// Decrement a counter by delta (saturating at 0).
pub fn decrement_counter(conn: &Connection, name: &str, delta: u64) -> Result<()> {
    let current = read_counter(conn, name)?;
    set_counter(conn, name, current.saturating_sub(delta))
}

/// Allocate the next entry ID. Reads current, increments, returns old value.
pub fn next_entry_id(conn: &Connection) -> Result<u64> {
    let current = read_counter(conn, "next_entry_id")?;
    let id = if current == 0 { 1 } else { current };
    set_counter(conn, "next_entry_id", id + 1)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(
            "CREATE TABLE counters (name TEXT PRIMARY KEY, value INTEGER NOT NULL);",
        )
        .expect("create counters");
        conn
    }

    #[test]
    fn read_counter_missing_returns_zero() {
        let conn = setup_conn();
        assert_eq!(read_counter(&conn, "nonexistent").unwrap(), 0);
    }

    #[test]
    fn set_and_read_counter_round_trip() {
        let conn = setup_conn();
        set_counter(&conn, "test_key", 42).unwrap();
        assert_eq!(read_counter(&conn, "test_key").unwrap(), 42);
    }

    #[test]
    fn increment_counter_from_zero() {
        let conn = setup_conn();
        increment_counter(&conn, "key", 5).unwrap();
        assert_eq!(read_counter(&conn, "key").unwrap(), 5);
    }

    #[test]
    fn increment_counter_accumulates() {
        let conn = setup_conn();
        set_counter(&conn, "key", 10).unwrap();
        increment_counter(&conn, "key", 3).unwrap();
        assert_eq!(read_counter(&conn, "key").unwrap(), 13);
    }

    #[test]
    fn decrement_counter_saturates_at_zero() {
        let conn = setup_conn();
        set_counter(&conn, "key", 2).unwrap();
        decrement_counter(&conn, "key", 5).unwrap();
        assert_eq!(read_counter(&conn, "key").unwrap(), 0);
    }

    #[test]
    fn next_entry_id_sequential() {
        let conn = setup_conn();
        assert_eq!(next_entry_id(&conn).unwrap(), 1);
        assert_eq!(next_entry_id(&conn).unwrap(), 2);
        assert_eq!(next_entry_id(&conn).unwrap(), 3);
    }

    #[test]
    fn next_entry_id_starts_at_one_when_zero() {
        let conn = setup_conn();
        set_counter(&conn, "next_entry_id", 0).unwrap();
        assert_eq!(next_entry_id(&conn).unwrap(), 1);
    }
}
