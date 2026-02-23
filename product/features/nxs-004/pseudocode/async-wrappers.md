# Pseudocode: async-wrappers

## Purpose
Provide feature-gated async wrappers using tokio::task::spawn_blocking.

## New File: crates/unimatrix-core/src/async_wrappers.rs

Feature-gated: only compiled when `async` feature is enabled.

### AsyncEntryStore

```
use std::sync::Arc;
use crate::error::CoreError;
use crate::traits::EntryStore;

pub struct AsyncEntryStore<T: EntryStore + 'static> {
    inner: Arc<T>,
}

impl<T: EntryStore + 'static> AsyncEntryStore<T> {
    pub fn new(inner: Arc<T>) -> Self {
        AsyncEntryStore { inner }
    }

    pub async fn insert(&self, entry: NewEntry) -> Result<u64, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.insert(entry))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn get(&self, id: u64) -> Result<EntryRecord, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.get(id))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    // ... same pattern for all EntryStore methods:
    // clone Arc, spawn_blocking, await, map JoinError
}
```

### AsyncVectorStore

```
pub struct AsyncVectorStore<T: VectorStore + 'static> {
    inner: Arc<T>,
}

// Same pattern: new(), async methods that clone Arc and spawn_blocking
```

### AsyncEmbedService

```
pub struct AsyncEmbedService<T: EmbedService + 'static> {
    inner: Arc<T>,
}

// Same pattern
```

### Note on ownership

For methods that take owned arguments (like insert(NewEntry)):
- NewEntry is moved into the closure: `move || inner.insert(entry)`

For methods that take references (like query_by_topic(&str)):
- Must clone/own the data: `let topic = topic.to_string(); spawn_blocking(move || inner.query_by_topic(&topic))`

For methods with slice arguments (like query_by_tags(&[String])):
- Must clone: `let tags = tags.to_vec(); spawn_blocking(move || inner.query_by_tags(&tags))`

## Error Handling
- JoinError from tokio (task panic, runtime shutdown) -> CoreError::JoinError(message)
- Inner method errors propagated via `?` after JoinError mapping

## Key Test Scenarios
- Async insert returns same ID as sync insert
- Async get returns same entry as sync get
- Async error propagation: get(999) returns CoreError::Store(EntryNotFound)
- JoinError conversion path (tested via direct construction)
