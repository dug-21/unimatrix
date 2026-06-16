//! Counter helpers for the COUNTERS table.
//!
//! All functions accept `&mut sqlx::SqliteConnection` as an executor. Callers
//! must hold the write connection inside a transaction for atomic
//! read-modify-write sequences (e.g., `next_entry_id`).
//!
//! Using `&mut SqliteConnection` (rather than `A: Acquire<'c>`) avoids the
//! "not general enough" lifetime error that occurs when these helpers are
//! called from `async fn` bodies that must be `Send` (e.g., `tokio::spawn`).

use sqlx::{Executor, Sqlite, SqliteConnection};

use crate::error::{Result, StoreError};

// ---------------------------------------------------------------------------
// Well-known counter name constants
// ---------------------------------------------------------------------------

/// Counter name for the audit log event ID sequence.
///
/// Live databases use `"next_audit_id"`. Code that formerly used
/// `"next_audit_event_id"` (introduced by nxs-011, fixed by #587) caused
/// UNIQUE constraint failures because the phantom counter row started at 0
/// while the live table had rows with event_id > 0.
pub const AUDIT_EVENT_COUNTER: &str = "next_audit_id";

/// Counter name for the Surface A compaction-event INSERT failure count (crt-054,
/// ADR-007 §6). Incremented by the server-side store_ops wrapper when
/// `insert_compaction_event` fails, so crt-055 can cross-check row-count vs
/// `increment_compaction` drift at review. Content-free: the name is a fixed
/// literal and the value is a pure count — no session_id/bytes appear here.
pub const COMPACTION_EVENTS_INSERT_FAILED: &str = "compaction_events_insert_failed";

// ---------------------------------------------------------------------------
// Public counter helpers (async)
// ---------------------------------------------------------------------------

/// Read a counter value. Returns 0 if the counter row does not exist.
pub async fn read_counter<'c, E>(executor: E, name: &str) -> Result<u64>
where
    E: Executor<'c, Database = Sqlite>,
{
    let val: Option<i64> = sqlx::query_scalar("SELECT value FROM counters WHERE name = ?1")
        .bind(name)
        .fetch_optional(executor)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;
    Ok(val.unwrap_or(0) as u64)
}

/// Set a counter to a specific value (upsert).
pub async fn set_counter<'c, E>(executor: E, name: &str, value: u64) -> Result<()>
where
    E: Executor<'c, Database = Sqlite>,
{
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)")
        .bind(name)
        .bind(value as i64)
        .execute(executor)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;
    Ok(())
}

/// Increment a counter by delta.
///
/// Takes `&mut SqliteConnection` directly to avoid "not general enough" lifetime
/// errors when the caller's future must be `Send` (e.g., inside `tokio::spawn`).
pub async fn increment_counter(conn: &mut SqliteConnection, name: &str, delta: u64) -> Result<()> {
    let current = read_counter(&mut *conn, name).await?;
    set_counter(&mut *conn, name, current + delta).await
}

/// Decrement a counter by delta (saturating at 0).
///
/// Takes `&mut SqliteConnection` directly to avoid "not general enough" lifetime
/// errors when the caller's future must be `Send` (e.g., inside `tokio::spawn`).
pub async fn decrement_counter(conn: &mut SqliteConnection, name: &str, delta: u64) -> Result<()> {
    let current = read_counter(&mut *conn, name).await?;
    set_counter(&mut *conn, name, current.saturating_sub(delta)).await
}

/// Atomically allocate the next value for a named counter.
///
/// Uses an upsert-or-increment form that is atomic within SQLite regardless
/// of transaction isolation level, eliminating the TOCTOU race present in
/// the two-step read_counter + set_counter pattern (bugfix-584).
///
/// Seeds the counter at 1 on first call (row absent or value=0 handled by
/// the upsert INSERT path which always starts at seed=1).
///
/// Takes `&mut SqliteConnection` directly to avoid "not general enough" lifetime
/// errors when the caller's future must be `Send` (e.g., inside `tokio::spawn`).
pub async fn next_counter(conn: &mut SqliteConnection, name: &str) -> Result<u64> {
    let val: Option<i64> = sqlx::query_scalar(
        "INSERT INTO counters (name, value) VALUES (?1, 1)
         ON CONFLICT(name) DO UPDATE SET value = value + 1
         RETURNING value",
    )
    .bind(name)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| StoreError::Database(e.into()))?;

    val.map(|v| v as u64)
        .ok_or_else(|| StoreError::Database("next_counter RETURNING returned no row".into()))
}

/// Allocate the next entry ID within an open transaction.
///
/// Atomically increments the `next_entry_id` counter and returns the allocated
/// value. The returned value is always >= 1 (first call returns 1).
///
/// Takes `&mut SqliteConnection` directly to avoid "not general enough" lifetime
/// errors when the caller's future must be `Send` (e.g., inside `tokio::spawn`).
pub async fn next_entry_id(conn: &mut SqliteConnection) -> Result<u64> {
    next_counter(conn, "next_entry_id").await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePool;

    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open in-memory");
        sqlx::query("CREATE TABLE counters (name TEXT PRIMARY KEY, value INTEGER NOT NULL)")
            .execute(&pool)
            .await
            .expect("create counters");
        pool
    }

    #[tokio::test]
    async fn test_read_counter_missing_returns_zero() {
        let pool = setup_pool().await;
        assert_eq!(read_counter(&pool, "nonexistent").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_set_and_read_counter_round_trip() {
        let pool = setup_pool().await;
        set_counter(&pool, "test_key", 42).await.unwrap();
        assert_eq!(read_counter(&pool, "test_key").await.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_increment_counter_from_zero() {
        let pool = setup_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        increment_counter(&mut conn, "key", 5).await.unwrap();
        assert_eq!(read_counter(&pool, "key").await.unwrap(), 5);
    }

    #[tokio::test]
    async fn test_increment_counter_accumulates() {
        let pool = setup_pool().await;
        set_counter(&pool, "key", 10).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        increment_counter(&mut conn, "key", 3).await.unwrap();
        assert_eq!(read_counter(&pool, "key").await.unwrap(), 13);
    }

    #[tokio::test]
    async fn test_decrement_counter_saturates_at_zero() {
        let pool = setup_pool().await;
        set_counter(&pool, "key", 2).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        decrement_counter(&mut conn, "key", 5).await.unwrap();
        assert_eq!(read_counter(&pool, "key").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_next_entry_id_sequential() {
        let pool = setup_pool().await;
        // Seed at 0 — matches db.rs init. First next_counter call returns 1.
        sqlx::query("INSERT INTO counters (name, value) VALUES ('next_entry_id', 0)")
            .execute(&pool)
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(next_entry_id(&mut conn).await.unwrap(), 1);
        assert_eq!(next_entry_id(&mut conn).await.unwrap(), 2);
        assert_eq!(next_entry_id(&mut conn).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_next_entry_id_starts_at_one_when_zero() {
        let pool = setup_pool().await;
        set_counter(&pool, "next_entry_id", 0).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(next_entry_id(&mut conn).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_next_counter_sequential_allocations() {
        let pool = setup_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(next_counter(&mut conn, "test_seq").await.unwrap(), 1);
        assert_eq!(next_counter(&mut conn, "test_seq").await.unwrap(), 2);
        assert_eq!(next_counter(&mut conn, "test_seq").await.unwrap(), 3);
    }

    /// Regression guard for bugfix-584: two concurrent next_counter calls on the
    /// same counter name must produce distinct values even with max_connections=2.
    ///
    /// Reproduces the TOCTOU race: with BEGIN DEFERRED and read+write two-step,
    /// both tasks could read the same snapshot. The atomic upsert form eliminates
    /// this — SQLite serializes the upsert internally.
    #[tokio::test]
    async fn test_next_counter_concurrent_no_duplicate() {
        use sqlx::sqlite::SqlitePoolOptions;
        use std::collections::HashSet;

        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect("sqlite::memory:")
            .await
            .expect("open in-memory pool with max_connections=2");

        sqlx::query("CREATE TABLE counters (name TEXT PRIMARY KEY, value INTEGER NOT NULL)")
            .execute(&pool)
            .await
            .expect("create counters table");

        let pool1 = pool.clone();
        let pool2 = pool.clone();

        let t1 = tokio::spawn(async move {
            let mut conn = pool1.acquire().await.expect("acquire conn 1");
            next_counter(&mut conn, "concurrent_test")
                .await
                .expect("next_counter task 1")
        });
        let t2 = tokio::spawn(async move {
            let mut conn = pool2.acquire().await.expect("acquire conn 2");
            next_counter(&mut conn, "concurrent_test")
                .await
                .expect("next_counter task 2")
        });

        let id1 = t1.await.expect("task 1 joined");
        let id2 = t2.await.expect("task 2 joined");

        let ids: HashSet<u64> = [id1, id2].into_iter().collect();
        assert_eq!(
            ids.len(),
            2,
            "concurrent next_counter calls must return distinct values, got id1={id1} id2={id2}"
        );
    }
}
