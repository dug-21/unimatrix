# Pseudocode: C4 EntryStore Trait Extension

## File: crates/unimatrix-core/src/traits.rs

### EntryStore Trait Extension

Add one method to EntryStore trait:

```
pub trait EntryStore: Send + Sync {
    // ... existing methods ...

    /// Record access for a batch of entry IDs.
    /// Updates access_count and last_accessed_at for each entry.
    fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError>;
}
```

Object safety preserved: &self, no generics, concrete return type.

## File: crates/unimatrix-core/src/adapters.rs

### StoreAdapter Implementation

Delegate to Store::record_usage with access_ids = entry_ids and empty vote slices:

```
impl EntryStore for StoreAdapter {
    // ... existing methods ...

    fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError> {
        Ok(self.inner.record_access(entry_ids, entry_ids, &[], &[], &[], &[])?)
    }
}
```

Note: At the trait level, all IDs get both access_count and last_accessed_at.
The server layer handles dedup before calling the raw store method directly.

## File: crates/unimatrix-core/src/async_wrappers.rs

### AsyncEntryStore Extension

Add record_access async wrapper matching established pattern:

```
impl<T: EntryStore + 'static> AsyncEntryStore<T> {
    // ... existing methods ...

    pub async fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError> {
        let inner = Arc::clone(&self.inner);
        let ids = entry_ids.to_vec();
        tokio::task::spawn_blocking(move || inner.record_access(&ids))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }
}
```
