use redb::{ReadableTable, WriteTransaction};

use crate::error::Result;
use crate::schema::COUNTERS;

/// Generate the next entry ID. Must be called within a write transaction.
///
/// The first entry ID is `1` (not `0`). ID `0` is reserved as a sentinel.
/// Reads the current value of `"next_entry_id"`, returns it, and stores
/// the incremented value.
pub(crate) fn next_entry_id(txn: &WriteTransaction) -> Result<u64> {
    let mut table = txn.open_table(COUNTERS)?;
    let current = match table.get("next_entry_id")? {
        Some(guard) => guard.value(),
        None => 1, // First ID is 1 (not 0)
    };
    table.insert("next_entry_id", current + 1)?;
    Ok(current)
}

/// Read a named counter value within the given write transaction.
/// Returns 0 if the counter key does not exist.
///
/// Reserved for future conditional-write patterns (e.g., check counter before insert).
#[allow(dead_code)]
pub(crate) fn read_counter_in_txn(txn: &WriteTransaction, key: &str) -> Result<u64> {
    let table = txn.open_table(COUNTERS)?;
    match table.get(key)? {
        Some(guard) => Ok(guard.value()),
        None => Ok(0),
    }
}

/// Increment a named counter within a write transaction.
/// Creates the counter with the given delta if it doesn't exist.
pub(crate) fn increment_counter(txn: &WriteTransaction, key: &str, delta: u64) -> Result<()> {
    let mut table = txn.open_table(COUNTERS)?;
    let current = match table.get(key)? {
        Some(guard) => guard.value(),
        None => 0,
    };
    table.insert(key, current + delta)?;
    Ok(())
}

/// Decrement a named counter within a write transaction.
/// Uses saturating subtraction to prevent underflow.
pub(crate) fn decrement_counter(txn: &WriteTransaction, key: &str, delta: u64) -> Result<()> {
    let mut table = txn.open_table(COUNTERS)?;
    let current = match table.get(key)? {
        Some(guard) => guard.value(),
        None => 0,
    };
    table.insert(key, current.saturating_sub(delta))?;
    Ok(())
}
