## ADR-003: Independent Adaptation State Persistence

### Context

Adaptation state must be saved and loaded alongside other persistent state (redb database, HNSW index). The question is whether adaptation state should live in a redb table, in the HNSW dump directory, or as an independent file.

Risk SR-06 identified: coupling adaptation persistence to HNSW persistence means failure of one could block or corrupt the other.

### Decision

Adaptation state persists in its **own independent file** (`adaptation.state`) in the same project data directory as the HNSW dump and redb database. The server handles each persistence concern independently:

1. On startup: load redb (required), load HNSW (required), load adaptation state (optional -- missing means fresh identity transform)
2. On shutdown: save redb (compact), save HNSW (dump), save adaptation state (save)
3. On maintenance: save adaptation state (debounced, not every training step)

If adaptation state loading fails (corrupt file, version mismatch), the server logs a warning and starts with fresh identity adaptation. Existing HNSW index entries are inconsistent with fresh adaptation until maintenance re-indexes them.

### Consequences

- **Easier**: Adaptation state failure never blocks server startup. Graceful degradation to unadapted embeddings.
- **Easier**: No schema migration required. No new redb tables.
- **Easier**: Adaptation state can be deleted independently to "reset" adaptation without losing entries.
- **Harder**: Three independent persistence concerns to coordinate during shutdown (but they are already independent -- redb and HNSW are already separate).
