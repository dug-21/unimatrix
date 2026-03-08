## ADR-001: MetricVector Types in unimatrix-store

### Context

`MetricVector`, `UniversalMetrics`, and `PhaseMetrics` are currently defined in `unimatrix-observe/src/types.rs`. The nxs-009 normalization requires the store to have a typed API (`store_metrics(&str, &MetricVector)`). Three options:

1. **Move types to `unimatrix-core`** — follows the col-013 `ObservationRecord` precedent, but `unimatrix-core` depends on `unimatrix-store`, so `unimatrix-store` cannot import from core (circular dependency).
2. **Move types to `unimatrix-store`** — follows the `EntryRecord` pattern. Store defines its own domain types. Core and observe re-export.
3. **Keep `&[u8]` API** — store stays generic, typed wrappers at server layer. Loses the benefit of typed read/write in the store itself.

### Decision

Option 2: Define `MetricVector`, `UniversalMetrics`, and `PhaseMetrics` in `unimatrix-store/src/metrics.rs`. Re-export from `unimatrix-observe/src/types.rs` and `unimatrix-core/src/lib.rs` for backward compatibility.

This follows the established pattern: `EntryRecord` is defined in store, re-exported by core. `ObservationRecord` was moved to core because core could import it (core depends on store). MetricVector must go to store because store needs it for the typed API and store is the leaf crate.

### Consequences

- **Easier**: Store has direct access to types for its SQL read/write methods. No new crate dependencies. Same pattern as EntryRecord. Zero-disruption re-exports maintain all existing import paths from observe.
- **Harder**: Metric types conceptually belong to the observation domain, not storage. Future metric type changes require editing the store crate. Acceptable tradeoff given the precedent.
