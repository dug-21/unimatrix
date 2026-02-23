## ADR-005: Shutdown via Arc::try_unwrap

### Context

The server shares `Store` via `Arc<Store>` across multiple subsystems (adapters, async wrappers, registry, audit log). `Store::compact()` requires `&mut self`. The vector index's `dump()` takes `&self` and works through `Arc`.

Options:
1. **Arc::try_unwrap**: Drop all clones, then unwrap to get owned Store, call compact
2. **Mutex<Store>**: Interior mutability, lock for compact only
3. **Skip compact**: Rely on redb's crash safety; compact is an optimization, not correctness
4. **UnsafeCell**: Raw interior mutability -- violates `#![forbid(unsafe_code)]`

### Decision

Use `Arc::try_unwrap()` with graceful degradation.

Shutdown sequence:
1. `VectorIndex::dump(&vector_dir)` -- works through Arc since dump takes &self
2. Drop the MCP server (drops all cloned Arcs in UnimatrixServer)
3. Drop async wrappers, adapters, registry, audit log (all hold Arc<Store> clones)
4. `Arc::try_unwrap(store)` -- should now be the sole owner
5. If Ok: call `compact()`, then drop
6. If Err: log warning "skipping compact: outstanding Store references", continue exit

The server architecture maintains a `LifecycleHandles` struct that holds the "lifecycle" `Arc<Store>` and `Arc<VectorIndex>` -- these are the references used for dump/compact and are the last to be dropped.

### Consequences

- **Easier:** Clean ownership semantics. No Mutex contention during normal operation. compact() runs only when the server has fully stopped.
- **Easier:** Graceful degradation -- if try_unwrap fails (bug: leaked Arc), the server still exits cleanly. redb is crash-safe; skipping compact just means the database file is slightly larger.
- **Harder:** Requires careful Arc lifecycle management. All subsystems must be dropped before try_unwrap. The shutdown coordinator must own the ordering.
- **Not affected:** dump() works through Arc, so vector persistence is unaffected by this decision.
