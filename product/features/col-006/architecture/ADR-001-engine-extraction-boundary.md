## ADR-001: Engine Extraction Boundary and Re-Export Strategy

### Context

col-006 introduces a `unimatrix-engine` crate to hold shared business logic that both the MCP tool handlers (stdio) and the UDS hook handlers need. The modules to extract are `confidence.rs`, `coaccess.rs`, and `project.rs` from `unimatrix-server`.

The extraction changes the dependency graph: `unimatrix-server` depends on `unimatrix-engine`, which depends on `unimatrix-core` and `unimatrix-store`. This is the highest-risk change in col-006 (SR-01 in the risk assessment) because incorrect extraction could silently break confidence scoring, co-access boosting, or project discovery for all 10 MCP tools.

The project has 1025 unit tests and 174 integration tests. Many integration tests import directly from `unimatrix_server::confidence`, `unimatrix_server::coaccess`, and `unimatrix_server::project`. Changing these imports across 174 test files is error-prone and unnecessary if the server re-exports from engine.

### Decision

**Extraction order:** Move one module at a time in dependency order. Run the full 1199-test suite after each move.

1. `project.rs` first — no dependencies on other server modules. Only depends on `sha2`, `dirs`, and `std`. After move, extend `ProjectPaths` with `socket_path: PathBuf`.

2. `confidence.rs` second — depends on `unimatrix_core::EntryRecord` and `unimatrix_core::Status`. One cross-reference to `coaccess::MAX_MEANINGFUL_PARTNERS` in `co_access_affinity()`. During the intermediate state (confidence moved, coaccess not yet moved), this cross-reference resolves by having engine depend on nothing from server. Instead, move the `MAX_MEANINGFUL_PARTNERS` constant to engine alongside confidence, and have coaccess reference it from engine once it also moves.

3. `coaccess.rs` third — depends on `unimatrix_store::Store` for `get_co_access_partners()` and on engine's confidence constants. After this move, all three modules are in engine.

**Re-export strategy:** After each module moves to `unimatrix-engine`, add a `pub use` re-export in `unimatrix-server/src/lib.rs`:

```rust
pub use unimatrix_engine::confidence;
pub use unimatrix_engine::coaccess;
pub use unimatrix_engine::project;
```

This preserves all existing import paths. Integration tests that use `unimatrix_server::confidence::compute_confidence` continue to work without modification.

**No behavioral changes during extraction.** Function signatures, constants, algorithms, and return types remain identical. The only change is the module's crate location. Any refactoring (renaming, restructuring, optimization) is explicitly out of scope during extraction.

**Engine crate dependencies:**
- `unimatrix-core` (for `EntryRecord`, `Status`)
- `unimatrix-store` (for `Store`, used by `coaccess::compute_boost_internal`)
- `serde`, `serde_json` (for wire protocol types)
- `sha2`, `dirs` (for project hash computation, from `project.rs`)
- `tracing` (for warning logs in coaccess)

### Consequences

**Easier:**
- Both MCP handlers and UDS handlers can call the same confidence/coaccess/project logic without code duplication.
- Future crates (e.g., a standalone hook binary in Phase 2) can depend on `unimatrix-engine` without pulling in the full server.
- Re-exports ensure zero test modifications during extraction.

**Harder:**
- Adding a crate to the workspace increases compilation time (incremental builds mitigate this).
- The `coaccess.rs` module depends on `unimatrix-store::Store` directly (not through a trait), which couples the engine to the concrete store implementation. This is acceptable for now — the Store API is stable, and adding a trait abstraction would be premature.
- `confidence::co_access_affinity` references `coaccess::MAX_MEANINGFUL_PARTNERS`. This creates a circular reference if the modules are in different crates. Solution: both modules move to the same engine crate, resolving the reference within the crate.
