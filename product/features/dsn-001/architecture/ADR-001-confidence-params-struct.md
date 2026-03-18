## ADR-001: ConfidenceParams Struct for Engine API

### Context

`freshness_score()` in `unimatrix-engine/src/confidence.rs` uses a compiled constant
`FRESHNESS_HALF_LIFE_HOURS = 168.0`. The W0-3 config feature must make this value
operator-configurable. The two candidates are:

**Option A — bare parameter**: Add `freshness_half_life_hours: f64` directly to
`freshness_score(last_accessed_at, created_at, now, freshness_half_life_hours)` and
thread the same value into `compute_confidence(entry, now, alpha0, beta0,
freshness_half_life_hours)`.

- Touches ~15 call sites in engine tests (tests hard-code `FRESHNESS_HALF_LIFE_HOURS`
  or call `freshness_score` with positional args).
- Produces a clean two-parameter cross-product: `(alpha0, beta0)` for helpfulness,
  `freshness_half_life_hours` for freshness.
- When W3-1 adds learned weights it will add more parameters, causing a second API
  break of similar magnitude.

**Option B — ConfidenceParams struct**: Introduce a context struct:

```rust
pub struct ConfidenceParams {
    pub freshness_half_life_hours: f64,
    pub alpha0: f64,
    pub beta0: f64,
}

impl Default for ConfidenceParams {
    fn default() -> Self {
        ConfidenceParams {
            freshness_half_life_hours: FRESHNESS_HALF_LIFE_HOURS,
            alpha0: COLD_START_ALPHA,
            beta0: COLD_START_BETA,
        }
    }
}
```

`compute_confidence(entry, now, params: &ConfidenceParams)` and
`freshness_score(last_accessed_at, created_at, now, params: &ConfidenceParams)`.

- All existing call sites become `compute_confidence(entry, now, &ConfidenceParams::default())` —
  a mechanical substitution, same call-site count but no semantic change to defaults.
- W3-1 adds `learned_weights: Option<LearnedWeights>` to `ConfidenceParams` without touching
  any call site that uses `Default`.
- The `alpha0`/`beta0` parameters are absorbed into the struct, reducing positional arg
  count from 4 to 2.

**SR-02 directly mandates this decision**: "A struct is safer if W3-1 will eventually
replace these values."

The PRODUCT-VISION W3-1 section explicitly states that the GNN cold-start uses
config-defined weights. Without a struct, W3-1 will break the API again.

### Decision

Introduce `ConfidenceParams` in `unimatrix-engine/src/confidence.rs`.

Signature changes:
- `freshness_score(last_accessed_at, created_at, now, params: &ConfidenceParams)` —
  uses `params.freshness_half_life_hours` instead of the constant.
- `compute_confidence(entry, now, params: &ConfidenceParams)` — delegates to
  `freshness_score` with the same params; the existing `alpha0`/`beta0` positional
  args become `params.alpha0`/`params.beta0`.

The const `FRESHNESS_HALF_LIFE_HOURS` remains in the file as the documented default
backing `ConfidenceParams::default()`. It is no longer used directly in computation.

All existing call sites in the engine's own test suite migrate to
`ConfidenceParams::default()` except for tests that deliberately exercise non-default
half-life values, which construct `ConfidenceParams { freshness_half_life_hours: X,
..Default::default() }`.

The server passes `ConfidenceParams` constructed from `UnimatrixConfig` at the point
where confidence is computed (background tick, explicit refresh). It does not store
a `ConfidenceParams` on the server struct — it constructs one per-call from the
Arc-loaded config.

### Consequences

**Easier:**
- W3-1 adds `Option<LearnedWeights>` to `ConfidenceParams` with `None` default; zero
  call-site churn for callers that don't need learned weights.
- `alpha0`/`beta0` are no longer positional — callers that want to override one but
  not the other use struct update syntax.
- The operator-configurable parameter surface in `ConfidenceParams::default()` is
  self-documenting: anyone reading the struct sees all tunable knobs.

**Harder:**
- `freshness_score()` can no longer be called with a bare f64 — callers that want a
  one-off half-life value must construct a `ConfidenceParams`. This is a minor
  ergonomic cost for a function that should only be called through the configured
  pipeline anyway.
- The test migration from positional `(alpha0, beta0)` args to `&ConfidenceParams`
  touches every confidence test. This is mechanical and non-risky but adds review
  surface area.
