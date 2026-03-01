## ADR-001: Observe Crate Independence

### Context

col-002 introduces a new `unimatrix-observe` crate for telemetry analysis. The crate needs access to JSONL files (filesystem I/O) and produces structured types (MetricVector, RetrospectiveReport). It does NOT need to read or write the redb database, embed entries, or interact with the MCP protocol.

The SCOPE.md explicitly states: "No dependency on `unimatrix-store` or `unimatrix-server`. Pure computation library." This constraint exists to keep the analysis engine testable in isolation and to prevent the observation subsystem from coupling to the knowledge engine's internal types.

The risk strategist flagged this boundary as SR-03 -- accidental coupling through shared types is a realistic risk when both crates deal with serialization and feature attribution.

### Decision

`unimatrix-observe` depends only on workspace dependencies (`serde`, `bincode`) and `std`. It defines its own types for observation records, metric vectors, hotspot findings, and reports. The server crate depends on `unimatrix-observe` and handles:
- Passing file paths and directory paths to the observe crate
- Receiving structured results from the observe crate
- Serializing MetricVector to bytes for store persistence
- Deserializing MetricVector from bytes on retrieval

The store crate treats MetricVector data as opaque `&[u8]` -- it never imports types from `unimatrix-observe`.

### Consequences

- **Easier**: Testing the analysis engine in isolation. Adding new detection rules without touching the server. Running analysis on JSONL files outside the MCP context.
- **Harder**: Any type that both crates need (e.g., feature cycle string conventions) must be duplicated or kept as primitive types (strings). MetricVector serialization must be consistent between the observe crate (which defines the struct) and the server crate (which passes bytes to the store).
