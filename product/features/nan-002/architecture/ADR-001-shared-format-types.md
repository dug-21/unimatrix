## ADR-001: Shared Format Types Between Export and Import

### Context

The export format contract (format_version 1, 8 tables, 26 entry columns) is defined implicitly by the nan-001 export code in `export.rs`. Export uses `serde_json::Value` construction with explicit column-to-JSON mapping (nan-001 ADR-002, Unimatrix #1098). Import must deserialize the same format. If either side changes independently, the contract breaks silently (SR-08).

The nan-001 export deliberately avoided Rust struct intermediaries to prevent coupling the wire format to internal type representations. This was correct for serialization. But the deserialization side (import) needs typed structs for safe parsing, and those structs are the natural place to document the format contract.

### Decision

Introduce a shared `format.rs` module in `crates/unimatrix-server/src/` containing typed deserialization structs for the JSONL format_version 1 contract:

- `ExportHeader` -- header line with `schema_version`, `exported_at`, `entry_count`, `format_version`
- `ExportRow` -- tagged enum with `#[serde(tag = "_table")]` discriminator over the 8 table types
- Per-table row structs (`CounterRow`, `EntryRow`, `EntryTagRow`, `CoAccessRow`, `FeatureEntryRow`, `OutcomeIndexRow`, `AgentRegistryRow`, `AuditLogRow`) with fields matching the JSON keys from export

Export continues to use `serde_json::Value` for serialization (preserving nan-001 ADR-002). The shared types serve as compile-time documentation of the format contract and are the single source of truth for import deserialization. If a future export change adds or renames a column, the shared struct will fail to compile for import, surfacing the contract break at build time rather than at runtime.

The `_table` discriminator in JSON maps naturally to a serde internally-tagged enum. The `_header` line is parsed separately (it lacks a `_table` field), so `ExportHeader` is a standalone struct.

### Consequences

- **Easier**: Adding a new export column requires updating the format struct, which forces import to handle it. Format drift is caught at compile time.
- **Easier**: Import deserialization is type-safe with proper null handling via `Option<T>`.
- **Easier**: Unit tests for deserialization can use the struct types directly.
- **Harder**: Export and import are coupled through the format module. A format_version bump requires updating format.rs (but this is the correct coupling point).
- **No change**: Export serialization remains Value-based per nan-001 ADR-002. The format structs are consumed only by import.
