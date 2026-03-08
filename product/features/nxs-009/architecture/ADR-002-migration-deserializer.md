## ADR-002: Self-Contained Migration Deserializer

### Context

The v8→v9 migration must read existing bincode blobs from `observation_metrics.data` and decompose them into SQL columns. The current `deserialize_metric_vector()` in `unimatrix-observe` uses `bincode::serde::decode_from_slice` with `bincode::config::standard()`. Two options:

1. **Import from observe** — add `unimatrix-observe` as a dependency of `unimatrix-store` (or use it transitively). Creates a new dependency edge.
2. **Self-contained deserializer** — add a `deserialize_metric_vector_v8()` function to `unimatrix-store/src/migration_compat.rs`, the same module that already contains `deserialize_entry_v5`, `deserialize_co_access_v5`, etc.

### Decision

Option 2: Self-contained deserializer in `migration_compat.rs`. The function defines its own serde-compatible structs (mirroring the v8 MetricVector layout) and uses `bincode::serde::decode_from_slice` with `bincode::config::standard()`. unimatrix-store already has bincode as a dependency.

### Consequences

- **Easier**: No new dependency edges. Migration is self-contained and frozen — it will never need to track future MetricVector changes. Follows the exact pattern of the nxs-008 migration deserializers.
- **Harder**: Duplicates the struct definitions for migration purposes. Acceptable — migration compat structs are snapshots of historical formats and intentionally decoupled from live types.
