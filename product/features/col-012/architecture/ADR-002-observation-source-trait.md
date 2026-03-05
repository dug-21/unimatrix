## ADR-002: ObservationSource Trait Preserves unimatrix-observe Independence

### Context

unimatrix-observe has no dependency on unimatrix-store (col-002 ADR-001). This independence allows the observe crate to be tested, compiled, and evolved without storage coupling.

col-012 requires the retrospective pipeline to read from SQLite instead of JSONL files. The simplest approach would add unimatrix-store as a dependency to unimatrix-observe, breaking this boundary.

Human directive: "I don't like breaking the architecture for convenience. There needs to be a solid business/technical reason why there should not be an abstraction."

### Decision

Define an `ObservationSource` trait in `unimatrix-observe::source`. The trait specifies what data the retrospective pipeline needs, not how to get it. `unimatrix-server` provides `SqlObservationSource` which implements the trait using `Arc<Store>`.

```rust
// unimatrix-observe defines the contract
pub trait ObservationSource {
    fn load_feature_observations(&self, feature_cycle: &str) -> Result<Vec<ObservationRecord>>;
    fn discover_sessions_for_feature(&self, feature_cycle: &str) -> Result<Vec<String>>;
    fn observation_stats(&self) -> Result<ObservationStats>;
}

// unimatrix-server implements it
pub struct SqlObservationSource { store: Arc<Store> }
impl ObservationSource for SqlObservationSource { ... }
```

The retrospective pipeline functions accept `&dyn ObservationSource` or `impl ObservationSource` instead of file paths.

### Consequences

- **Independence preserved**: unimatrix-observe continues to have zero dependency on unimatrix-store.
- **Testability improved**: Tests can provide mock/in-memory implementations of `ObservationSource` instead of creating JSONL files on disk.
- **Minimal trait surface**: Three methods cover all retrospective pipeline needs. No over-engineering.
- **Dependency direction**: observe defines the interface, server implements it. This is dependency inversion -- the higher-level policy (observe) does not depend on the lower-level detail (store).
- **Future flexibility**: If data source changes again (e.g., remote API), only the implementation changes. The observe crate is untouched.
