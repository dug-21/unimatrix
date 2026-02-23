## ADR-001: Core Crate as Trait Host

### Context

nxs-004 introduces trait abstractions over three existing crates (unimatrix-store, unimatrix-vector, unimatrix-embed). The traits need a home. Options:

1. Define traits in each concrete crate (store defines EntryStore, vector defines VectorStore, etc.)
2. Define all traits in unimatrix-store (since it's the lowest-level crate)
3. Create a new `unimatrix-core` crate that hosts all traits and re-exports domain types

Option 1 scatters the trait surface across crates. Consumers need three dependencies. Option 2 overloads unimatrix-store with responsibilities beyond storage. Option 3 creates a clean abstraction layer that becomes the single consumer-facing dependency.

### Decision

Create a new `unimatrix-core` crate at `crates/unimatrix-core/`. This crate:

- Defines all three core traits (`EntryStore`, `VectorStore`, `EmbedService`)
- Defines `CoreError` as the unified error type
- Re-exports domain types from lower crates (`EntryRecord`, `NewEntry`, `QueryFilter`, `Status`, `TimeRange`, `SearchResult`, etc.)
- Provides domain adapters (`StoreAdapter`, `VectorAdapter`, `EmbedAdapter`)
- Provides feature-gated async wrappers

Downstream consumers (vnc-001) only need `unimatrix-core` as a dependency.

### Consequences

- **Easier**: Single dependency for MCP server and future consumers. Clean separation between trait definitions and implementations.
- **Easier**: Adding new trait methods or adapters is localized to one crate.
- **Harder**: One more crate in the workspace to maintain. Re-exports must be kept in sync when lower crates change public types.
- **Neutral**: Does not create a circular dependency -- core depends on store/vector/embed, not the reverse.
