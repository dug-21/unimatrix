# Component Pseudocode: `AsyncEmbedService` Removal

**File to modify**: `crates/unimatrix-core/src/async_wrappers.rs`

---

## Purpose

Remove the `AsyncEmbedService` struct and its three methods from `unimatrix-core`.
This struct wraps `EmbedService` in `spawn_blocking`, placing execution scheduling
logic inside a domain crate — a crate boundary violation (ADR-001). It has zero
consumers in `unimatrix-server` (all server call sites use `EmbedAdapter` directly,
not `EmbedService` through `AsyncEmbedService`).

This is a dead-code removal, not a migration. No call site needs updating.

`AsyncVectorStore<T>` and all its methods remain unchanged. The `async` feature flag
and the tokio dependency in `unimatrix-core` are retained because `AsyncVectorStore`
requires them.

---

## What to Delete

### From `unimatrix-core/src/async_wrappers.rs`

Delete the entire `AsyncEmbedService` block (lines 87–121 in the current file):

```
// DELETE: this entire block

/// Async wrapper for any `EmbedService` implementation.
pub struct AsyncEmbedService<T: EmbedService + 'static> {
    inner: Arc<T>,
}

impl<T: EmbedService + 'static> AsyncEmbedService<T> {
    pub fn new(inner: Arc<T>) -> Self {
        AsyncEmbedService { inner }
    }

    pub async fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError> {
        let inner = Arc::clone(&self.inner);
        let title = title.to_string();
        let content = content.to_string();
        tokio::task::spawn_blocking(move || inner.embed_entry(&title, &content))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn embed_entries(
        &self,
        entries: Vec<(String, String)>,
    ) -> Result<Vec<Vec<f32>>, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.embed_entries(&entries))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn dimension(&self) -> usize {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.dimension())
            .await
            .unwrap_or(0)
    }
}
```

### Import cleanup in `async_wrappers.rs`

After the deletion, check whether `EmbedService` is still used in the file.
`EmbedService` was imported at line 10:
```
use crate::traits::{EmbedService, VectorStore};
```

If `AsyncEmbedService` is the only consumer of `EmbedService` in this file, remove
`EmbedService` from the import. `VectorStore` must be retained for `AsyncVectorStore`.

The resulting import should be:
```
use crate::traits::VectorStore;
```

If the compiler reports `EmbedService` still referenced elsewhere in the file, retain it.

---

## What to Retain

Everything else in `async_wrappers.rs` is unchanged:

- Module-level doc comment
- `use std::sync::Arc;`
- `use unimatrix_vector::SearchResult;`
- `use crate::error::CoreError;`
- `use crate::traits::VectorStore;` (after cleanup)
- `AsyncVectorStore<T>` struct definition
- All `AsyncVectorStore<T>` methods:
  - `new`
  - `insert`
  - `search`
  - `search_filtered`
  - `point_count`
  - `contains`
  - `stale_count`
  - `get_embedding`

The `async` feature flag in `unimatrix-core/Cargo.toml` is retained. The tokio
dependency in `unimatrix-core/Cargo.toml` is retained. `unimatrix-core` gains no
new dependencies from this feature.

---

## Public API Impact

### `unimatrix-core/src/lib.rs` — check re-exports

Verify whether `AsyncEmbedService` is re-exported from `unimatrix-core/src/lib.rs`.
If it is re-exported (e.g., via `pub use async_wrappers::AsyncEmbedService`), remove
that re-export line. If it is not explicitly re-exported, no change to `lib.rs` is needed.

Run `grep -r "AsyncEmbedService" crates/unimatrix-core/` after the deletion to confirm
zero references remain.

---

## Verification Steps

These are the verification steps to confirm the deletion is correct and complete:

### Step 1: Zero references in the workspace

```
grep -r "AsyncEmbedService" crates/
```

Must return zero results. If any result appears, the deletion is incomplete.

### Step 2: `cargo check --workspace` exits 0

```
cargo check --workspace
```

Must succeed with zero errors. Any compile error means a consumer was found that
was not caught by the grep — investigate and update that consumer as part of this feature.

### Step 3: `AsyncVectorStore` is present and unchanged

```
grep -r "AsyncVectorStore" crates/unimatrix-core/
```

Must return the struct definition line in `async_wrappers.rs` and any other files that
use it. No `AsyncVectorStore` lines should be removed.

### Step 4: `spawn_blocking` removed from `async_wrappers.rs`

```
grep -n "spawn_blocking" crates/unimatrix-core/src/async_wrappers.rs
```

Must return zero results. Before deletion, this file had two `spawn_blocking` calls
inside `AsyncEmbedService` (lines 100 and 110 in the current source). Both are removed
with the struct.

---

## State Machines

None. This is a pure deletion. No state transitions or initialization sequences.

---

## Error Handling

No error handling is added. The deletion removes error paths (the `JoinError` mapping
inside `embed_entry` and `embed_entries`). No replacement is needed because there are
zero consumers of `AsyncEmbedService` in the codebase.

---

## Key Test Scenarios (AC-05, R-05, NFR-07)

1. **Zero workspace references after deletion** (AC-05): `grep -r "AsyncEmbedService" crates/`
   returns zero results.

2. **Workspace build passes** (AC-05, NFR-07): `cargo check --workspace` exits 0. This
   is the primary enforcement mechanism for the deletion being complete and correct.

3. **`AsyncVectorStore` is present and unchanged** (AC-05 positive assertion):
   `grep -r "AsyncVectorStore" crates/unimatrix-core/` returns the struct definition.
   `AsyncVectorStore` methods compile successfully.

4. **`spawn_blocking` absent from `async_wrappers.rs`** (AC-05, R-06 secondary check):
   `grep -n "spawn_blocking" crates/unimatrix-core/src/async_wrappers.rs` returns zero results.

5. **`unimatrix-core` gains no new dependencies**: `cargo tree -p unimatrix-core` before and
   after the deletion shows identical dependency trees (or the tree shrinks if `EmbedService`
   import removal affects any transitive dep — but it won't; the trait is defined within core).

6. **Compilation fails on import attempt** (AC-05 structural): a test that attempts to import
   `AsyncEmbedService` from `unimatrix_core` must fail to compile. This is verified by
   `cargo check --workspace` catching any existing consumer; no explicit test is written
   for a non-existent type.
