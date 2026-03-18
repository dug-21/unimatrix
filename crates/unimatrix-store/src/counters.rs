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

/// Allocate the next entry ID within an open transaction.
///
/// Reads `next_entry_id`, increments it, returns the pre-increment value.
/// The returned value is always >= 1 (0 → 1 bootstrap).
///
/// Takes `&mut SqliteConnection` directly to avoid "not general enough" lifetime
/// errors when the caller's future must be `Send` (e.g., inside `tokio::spawn`).
pub async fn next_entry_id(conn: &mut SqliteConnection) -> Result<u64> {
    let current = read_counter(&mut *conn, "next_entry_id").await?;
    let id = if current == 0 { 1 } else { current };
    set_counter(&mut *conn, "next_entry_id", id + 1).await?;
    Ok(id)
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
        sqlx::query("INSERT INTO counters (name, value) VALUES ('next_entry_id', 1)")
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
}
