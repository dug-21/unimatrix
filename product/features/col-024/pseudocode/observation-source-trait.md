# Component: ObservationSource Trait
# File: crates/unimatrix-observe/src/source.rs

## Purpose

Add `load_cycle_observations` to the `ObservationSource` trait so that
`context_cycle_review` can dispatch the primary lookup path via `&dyn ObservationSource`
without depending on the concrete `SqlObservationSource` type.

The trait lives in `unimatrix-observe`, which has no dependency on `unimatrix-store` or
`tracing`. This component is purely a declaration change — one new method on the trait,
no implementation logic here.

## New/Modified Functions

### `ObservationSource::load_cycle_observations` (new trait method)

```
// unimatrix-observe/src/source.rs

pub trait ObservationSource {
    // --- EXISTING METHODS (unchanged) ---

    fn load_feature_observations(&self, feature_cycle: &str) -> Result<Vec<ObservationRecord>>;

    fn discover_sessions_for_feature(&self, feature_cycle: &str) -> Result<Vec<String>>;

    fn load_unattributed_sessions(&self) -> Result<Vec<ParsedSession>>;

    fn observation_stats(&self) -> Result<ObservationStats>;

    // --- NEW METHOD (col-024) ---

    /// Load observation records attributed to a named feature cycle via cycle_events timestamps.
    ///
    /// This is the primary attribution path introduced in col-024. It uses the
    /// cycle_events table (which records cycle_start / cycle_stop events synchronously)
    /// to derive time windows, then discovers sessions by matching the topic_signal column
    /// against the cycle_id within those windows.
    ///
    /// Returns Ok(vec![]) in two cases:
    ///   1. No cycle_events rows exist for cycle_id (pre-col-024 features).
    ///   2. cycle_events rows exist but no observations match topic_signal within windows.
    /// The caller must not treat Ok(vec![]) as an error -- the legacy fallback activates
    /// on this return value (FM-01).
    ///
    /// Returns Err(ObserveError) only on a genuine SQL or database failure.
    ///
    /// Sync contract: implementations must not use async fn. All async work must be
    /// bridged via block_sync inside the implementation body (NFR-01, ADR-001).
    fn load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>>;
}
```

## State Machines

None. This is a pure trait declaration. The trait has no lifecycle state.

## Initialization Sequence

None. Trait methods are declared, not instantiated.

## Data Flow

```
Input:  &str cycle_id          -- e.g. "col-024"
Output: Result<Vec<ObservationRecord>, ObserveError>
           Ok(vec![]) when no rows or no match
           Ok(records) when observations found
           Err(ObserveError::Database(_)) on SQL failure
```

The `ObservationRecord` type is defined in `unimatrix-observe::types`. It is not changed
by col-024.

## Error Handling

The method signature mirrors all other `ObservationSource` methods:
- Return type is `Result<Vec<ObservationRecord>>` which aliases
  `Result<Vec<ObservationRecord>, ObserveError>` via `unimatrix-observe::error::Result`.
- Implementations propagate database errors as `ObserveError::Database(msg)`.
- Empty result is `Ok(vec![])`, not `Err(...)`.

## Integration Risk

**I-01**: Any existing mock or test implementation of `ObservationSource` outside
`SqlObservationSource` must add this method or fail to compile. The implementation agent
must search for other `impl ObservationSource for` blocks in the workspace and add a
stub returning `Ok(vec![])` to each.

Search pattern: `impl ObservationSource for` across all workspace crates.
If found outside `unimatrix-server/src/services/observation.rs`, add:

```
fn load_cycle_observations(&self, _cycle_id: &str) -> Result<Vec<ObservationRecord>> {
    Ok(vec![])
}
```

## Key Test Scenarios

- **AC-10**: After the trait change, `cargo test` passes for `unimatrix-observe`
  (no test additions needed in the crate itself — trait declaration has no logic to test).
- **AC-10 (integration)**: `cargo test` passes for `unimatrix-server` — compilation
  verifies that `SqlObservationSource` satisfies the updated trait.

## Constraints

- No `tracing` import. `unimatrix-observe` does not depend on `tracing` (knowledge package).
- No `async fn`. Sync-trait contract is non-negotiable (NFR-01).
- The method must appear on the trait before `SqlObservationSource` can implement it.
