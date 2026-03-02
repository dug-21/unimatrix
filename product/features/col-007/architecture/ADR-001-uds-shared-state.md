## ADR-001: UDS Listener Shared State via Parameter Expansion

### Context

The UDS listener (`start_uds_listener()`) currently receives only `Arc<Store>` and dispatches requests with log-and-ack handlers. col-007's ContextSearch handler needs access to the embedding service, vector store, entry store, and adaptation service to run the search pipeline.

Two approaches were considered:

**Option A: Shared search function in unimatrix-engine.** Extract the search pipeline (~170 lines) from `tools.rs` into `unimatrix-engine/src/search.rs`. Both the MCP tool and UDS handler call the same function. This avoids code duplication but creates a coupling problem: the search function would need to accept `EmbedServiceHandle` (defined in `unimatrix-server`), `AdaptationService` (defined in `unimatrix-adapt`), and `AsyncEntryStore`/`AsyncVectorStore` (defined in `unimatrix-core`). Either the function takes trait objects (adding abstraction layers) or `unimatrix-engine` gains dependencies on server-specific types, violating the crate dependency direction (engine should not depend on server).

**Option B: Parameter expansion with duplicated orchestration.** Pass the additional services as individual Arc parameters to `start_uds_listener()`. Implement the search pipeline directly in the UDS dispatcher, calling the same underlying service methods. The orchestration code (~40 lines: embed -> adapt -> search -> fetch -> re-rank -> boost -> truncate) is duplicated between `tools.rs` and `uds_listener.rs`, but each calls the same service implementations.

The human flagged a preference for cleaner boundaries even if it means some duplication (SR-02 response).

### Decision

Use Option B: parameter expansion with duplicated orchestration.

`start_uds_listener()` accepts the additional services as individual Arc parameters:
- `embed_service: Arc<EmbedServiceHandle>`
- `vector_store: Arc<AsyncVectorStore<VectorAdapter>>`
- `entry_store: Arc<AsyncEntryStore<StoreAdapter>>`
- `adapt_service: Arc<AdaptationService>`

The search pipeline orchestration is implemented directly in `uds_listener.rs`. This duplicates the pipeline wiring (~40 lines) but not the business logic (embedding, HNSW search, re-ranking, co-access boost are all in their respective modules/crates).

No new crate dependencies. No shared search function in `unimatrix-engine`.

### Consequences

**Easier:**
- Crate boundaries remain clean. `unimatrix-engine` does not gain server-specific dependencies.
- Each transport (MCP, UDS) can evolve its search pipeline independently (e.g., UDS might later skip audit logging, apply different defaults).
- No risk of regression from extraction refactoring (SR-01 is eliminated).

**Harder:**
- If the search pipeline steps change (e.g., a new re-ranking factor), both `tools.rs` and `uds_listener.rs` must be updated. Drift is possible.
- The `start_uds_listener()` signature grows to 8 parameters. This is manageable but warrants monitoring -- if it grows further in col-008+, consider a context struct.

**Mitigations for drift:**
- Integration tests that verify MCP and UDS produce equivalent results for the same query will catch behavioral divergence.
- The underlying service calls (`embed_entry`, `search`, `rerank_score`, `compute_search_boost`) are unchanged and shared via their crates.
