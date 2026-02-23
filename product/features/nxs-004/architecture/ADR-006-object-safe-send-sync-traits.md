## ADR-006: Object-Safe Send+Sync Traits

### Context

Core traits must be usable as trait objects (`dyn EntryStore`) for dynamic dispatch in the MCP server. They must also be shareable across threads via `Arc<dyn EntryStore>`. Options:

1. Generic-only traits (no object safety constraint) -- `impl EntryStore`
2. Object-safe traits with `Send + Sync` supertrait bounds
3. Object-safe traits without `Send + Sync` (add bounds at usage sites)

Option 1 prevents `dyn` usage, forcing the MCP server to be generic over all three trait types (complex generic signatures). Option 3 makes `Arc<dyn EntryStore>` usage verbose (need `Arc<dyn EntryStore + Send + Sync>` everywhere). Option 2 is the standard Rust pattern for shared service traits.

### Decision

All three core traits use `Send + Sync` supertrait bounds:

```rust
pub trait EntryStore: Send + Sync {
    fn insert(&self, entry: NewEntry) -> Result<u64, CoreError>;
    fn get(&self, id: u64) -> Result<EntryRecord, CoreError>;
    // ... all methods take &self, return concrete types
}

pub trait VectorStore: Send + Sync {
    fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<(), CoreError>;
    fn search(&self, query: &[f32], top_k: usize, ef_search: usize) -> Result<Vec<SearchResult>, CoreError>;
    // ...
}

pub trait EmbedService: Send + Sync {
    fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError>;
    // ...
}
```

Object safety requirements:
- All methods take `&self` (not `self` or `&mut self` for object safety on most methods)
- Return types are concrete (no `impl Trait` returns)
- No generic method parameters

**Exception**: `EntryStore::compact(&mut self)` requires `&mut self`. This method is NOT part of the object-safe trait surface. It will be called directly on the concrete `Store` type during shutdown (vnc-001 coordinates shutdown and has access to the concrete type). The trait includes all query and mutation methods that take `&self` only.

### Consequences

- **Easier**: `Arc<dyn EntryStore>` works everywhere. MCP server stores one `Arc<dyn EntryStore>` instead of being generic.
- **Easier**: `EmbeddingProvider` from unimatrix-embed already uses this exact pattern (`Send + Sync` supertrait). Consistency across the codebase.
- **Harder**: `compact()` is excluded from the trait since it needs `&mut self`. Shutdown coordination must use the concrete `Store` type. This is acceptable since shutdown is an infrastructure concern, not a business logic concern.
- **Neutral**: All existing concrete types (`Store`, `VectorIndex`, `OnnxProvider`) are already `Send + Sync`.
