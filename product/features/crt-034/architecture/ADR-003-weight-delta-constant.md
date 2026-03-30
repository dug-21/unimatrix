## ADR-003: Weight Delta as Module-Private Constant, Not Config Field

### Context

The promotion tick must suppress unnecessary weight updates on edges whose normalized
weight has not changed meaningfully. SCOPE.md §Design Decision 1 specifies a delta
threshold of `0.1` as the churn-suppression guard.

Two options:
1. Expose the delta as an `InferenceConfig` field (e.g.,
   `co_access_weight_update_delta: f64`, default `0.1`, operator-configurable via TOML).
2. Hard-code it as a module-private constant `CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1`
   in `co_access_promotion_tick.rs`.

The config field pattern (`max_co_access_promotion_per_tick`) is appropriate for
parameters operators legitimately tune across deployments: throughput caps, model
thresholds, pool sizes. These parameters affect system behavior in deployment-specific
ways.

The weight delta is fundamentally different:
- It suppresses writes when the weight change is within floating-point noise or minor
  count drift. Its purpose is purely internal: avoid spurious SQLite writes.
- There is no domain-semantics reason an operator would want to configure this per
  deployment. Setting it to 0.0 would cause writes on every tick for every edge with any
  count change (wasteful). Setting it above 0.5 would prevent meaningful updates from
  reaching PPR (harmful).
- The correct value (`0.1`) represents "10% of the normalized weight scale" — a
  reasonable noise floor for the `[0.0, 1.0]` weight range. This is a calibrated
  engineering constant, not a domain policy knob.

The SCOPE.md author explicitly chose "named constant" over "config field" for this
reason.

### Decision

Implement `CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1` as a module-private constant
in `crates/unimatrix-server/src/services/co_access_promotion_tick.rs`.

It is NOT added to `InferenceConfig`. It is NOT exported to any other module.

**Type is `f64`, not `f32`.** SQLite `REAL` columns are fetched as `f64` by sqlx. When
the tick reads an existing edge weight from `GRAPH_EDGES`, it receives an `f64`. Comparing
against an `f32` constant after implicit cast introduces precision noise: `0.1f32` as
`f64` is `0.100000001490116...`. The confidence system already encountered this class of
bug in crt-005 (f32/f64 score divergence). `f64 = 0.1` avoids the issue entirely.

The constant's doc comment explains its purpose:
```rust
/// Minimum weight change required to UPDATE an existing CoAccess edge.
/// Suppresses spurious writes when count drift produces only minor weight changes.
/// Not operator-configurable: this is a calibrated noise floor, not a domain policy.
/// f64 to match SQLite REAL fetch type and avoid implicit cast precision noise.
const CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1;
```

### Consequences

- No TOML documentation, no serde plumbing, no validation, no merge logic needed for
  this parameter.
- Changing the delta requires a code change and re-deploy (acceptable: it is an
  engineering constant, changes are infrequent and deliberate).
- Operators cannot misconfigure it to `0.0` or `0.9` — both of which would be harmful.
- Consistent with the pattern established for internal constants throughout the codebase
  (e.g., `CONTRADICTION_SCAN_INTERVAL_TICKS`, `TICK_TIMEOUT` in `background.rs`).
