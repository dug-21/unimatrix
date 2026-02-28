## ADR-002: Bincode v2 for Adaptation State Persistence

### Context

The adaptation state (LoRA weights, Fisher diagonal, reference params, prototypes, training metadata) must be persisted to disk and loaded at startup. The state size ranges from ~50KB (rank=4) to ~1.8MB (rank=16) depending on configuration.

Options considered:
1. **bincode v2**: Already in the workspace (used by unimatrix-store for redb serialization). Fast, compact binary format. `serde(default)` enables forward compatibility.
2. **JSON/MessagePack**: Human-readable (JSON) or semi-compact (MessagePack). Higher overhead for f32 arrays. Not justified for internal state files.
3. **redb table**: Store adaptation state in a new redb table alongside existing tables. Couples adaptation to the store schema.
4. **Raw bytes with custom format**: Maximum control but maximum maintenance burden.

### Decision

Use **bincode v2 with serde derive** for adaptation state persistence. The `AdaptationState` struct derives `Serialize + Deserialize`. A `version: u32` field at the top of the struct enables format evolution. New fields use `#[serde(default)]` for forward compatibility (same pattern as `EntryRecord`).

The state file is a standalone file in the project's data directory (alongside HNSW dump files), not a redb table. This keeps adaptation state decoupled from the entry store schema.

### Consequences

- **Easier**: Consistent with project conventions (bincode v2 everywhere). No new serialization dependency.
- **Easier**: Forward-compatible evolution via `serde(default)` -- exactly the same pattern used for EntryRecord.
- **Easier**: Independent persistence -- adaptation state can fail without corrupting the entry store.
- **Harder**: Binary format is not human-inspectable (acceptable for internal state).
- **Harder**: Major version changes (e.g., changing rank) require state re-creation (acceptable -- training restarts from near-identity).
