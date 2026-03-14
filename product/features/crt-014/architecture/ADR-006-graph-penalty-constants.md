## ADR-006: Named Penalty Constants in graph.rs (Fixed for v1)

### Context

The `graph_penalty` function uses several coefficient values to derive topology-based penalties. These values are proposed in ASS-017 as reasonable starting points based on relative severity reasoning, not empirical data. The human chose (OQ-1 answer) to implement them as named `const` values in `graph.rs` — fixed for v1, not runtime-configurable.

Options considered:
1. **Hardcoded literals in `graph_penalty` body** — opaque, hard to locate for future tuning
2. **Named constants in `graph.rs`** — self-documenting, easy to locate and tune in future
3. **Runtime-configurable parameters** — adds complexity, not warranted until empirical evidence shows the values need tuning

### Decision

Implement all penalty coefficients as named `pub const` values in `unimatrix-engine/src/graph.rs`:

```rust
pub const ORPHAN_PENALTY: f64 = 0.75;            // deprecated, no successors
pub const CLEAN_REPLACEMENT_PENALTY: f64 = 0.40; // active terminal at depth 1
pub const HOP_DECAY_FACTOR: f64 = 0.60;          // multiplier per additional hop depth
pub const PARTIAL_SUPERSESSION_PENALTY: f64 = 0.60; // >1 active successor
pub const DEAD_END_PENALTY: f64 = 0.65;          // no active terminal reachable
pub const FALLBACK_PENALTY: f64 = 0.70;          // cycle detection fallback
pub const MAX_TRAVERSAL_DEPTH: usize = 10;        // DFS depth cap
```

These are `pub` so they can be referenced in integration tests (asserting ordering invariants without hardcoding the exact values in test assertions that would break on tuning).

No runtime configurability. No struct or config file parameter. When empirical data (e.g., A/B testing, user feedback through the helpfulness mechanism) indicates a constant needs adjustment, a code change to `graph.rs` with a comment referencing the evidence is the correct path.

### Consequences

Easier: Constants are self-documenting and locatable. `grep ORPHAN_PENALTY` finds all references. Behavioral ordering tests in `graph.rs` use the constants directly — a constant change doesn't break tests unless the ordering invariant itself changes. Future ADR updating these constants can reference the empirical evidence.

Harder: Adjusting penalties requires a code change and redeploy. There is no admin tool or config file override for penalty tuning. If rapid iteration on penalty values is needed (e.g., during a calibration sprint), a more dynamic approach would be required.
