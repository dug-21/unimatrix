## ADR-006: Preset Resolution Pipeline

### Context

Three risks drive this ADR:

**SR-02**: `ConfidenceParams` must carry all six weight values. Without injecting
them into `compute_confidence`, preset selection has zero effect on scoring. The
resolution pipeline is the mechanism that connects config load → preset enum →
populated `ConfidenceParams`.

**SR-11**: `[knowledge] freshness_half_life_hours` creates three possible sources of
truth for one value: the preset's built-in default, a `[knowledge]` override, and a
`[confidence]` field for `custom`. If the resolution order is not implemented as a
single site, an operator who sets `[knowledge] freshness_half_life_hours` with a
named preset may unknowingly have it silently ignored.

**SR-13**: W3-1 GNN cold-start must read the effective weight vector. If W3-1 begins
before `ConfidenceParams` carries all six weights, there will be a format conflict
or W3-1 will be blocked.

The pipeline must resolve these in a single, deterministic, auditable function.

**`freshness_half_life_hours` precedence chain (all combinations):**

| `[profile] preset` | `[knowledge] freshness_half_life_hours` | Effective value | Rationale |
|---|---|---|---|
| named (non-custom) | absent | Preset's built-in value | Pure preset |
| named (non-custom) | present | `[knowledge]` value | Explicit operator override |
| `custom` | absent | Required — abort startup | `custom` requires full specification |
| `custom` | present | `[knowledge]` value | `[knowledge]` is the one operator-interpretable field |

For `custom` with no `[knowledge] freshness_half_life_hours`, startup aborts:
"preset=custom requires freshness_half_life_hours in [knowledge] or a named preset".
The SCOPE.md AC-25 is slightly ambiguous here ("must also be specified there" — "there"
being the `[confidence]` section). This ADR clarifies: `freshness_half_life_hours`
belongs in `[knowledge]` (it is an operator-interpretable knowledge domain parameter),
not in `[confidence]` (which carries weights only). The `[confidence]` section has no
`freshness_half_life_hours` field; `[knowledge]` is the single location for this value.

**Resolution flow:**

```
load_config() returns UnimatrixConfig
        │
        ▼
resolve_confidence_params(config) → ConfidenceParams
        │
        ├─ preset = Collaborative → ConfidenceParams::default()
        │                           (all six weights from compiled constants)
        │
        ├─ preset = Authoritative|Operational|Empirical
        │           → ConfidenceParams { w_base, w_usage, w_fresh, w_help,
        │                                w_corr, w_trust } from ADR-005 table
        │           + freshness_half_life_hours:
        │               if config.knowledge.freshness_half_life_hours.is_some()
        │                   → config.knowledge.freshness_half_life_hours
        │               else → preset's built-in half_life from ADR-005 table
        │
        └─ preset = Custom
                    → validate confidence.weights is Some
                    → validate freshness_half_life_hours is in config.knowledge
                    → ConfidenceParams { w_base: weights.base, ...,
                                         freshness_half_life_hours: config.knowledge.freshness_half_life_hours }
```

The function signature:

```rust
/// Resolve the active ConfidenceParams from the loaded config.
///
/// This is the single resolution site for all confidence parameter sources.
/// Call once during startup; pass the result to compute_confidence().
///
/// SR-02: carries all six weights so preset selection affects scoring.
/// SR-11: single resolution site prevents half_life precedence confusion.
/// SR-13: the returned ConfidenceParams is the W3-1 cold-start vector.
pub fn resolve_confidence_params(config: &UnimatrixConfig) -> Result<ConfidenceParams, ConfigError>
```

`ConfigError` is returned for the `custom` + missing weights / half_life cases.
For named presets with a missing `[knowledge] freshness_half_life_hours`, the
function returns `Ok` using the preset's built-in value — no error, no warning
(absence is intentional).

**Placement**: `resolve_confidence_params` lives in
`unimatrix-server/src/infra/config.rs` alongside `UnimatrixConfig` (per ADR-002).

**`ConfidenceParams::from_preset` as a helper method:**

The `from_preset` helper constructs the params for a named preset using the
ADR-005 weight table. It is used by `resolve_confidence_params` internally and
by the SR-10 test:

```rust
impl ConfidenceParams {
    /// Construct ConfidenceParams from a named preset's built-in weight table.
    ///
    /// Does not apply [knowledge] freshness_half_life_hours override —
    /// that is handled by resolve_confidence_params(). This method produces
    /// the pure preset values for testing and documentation.
    pub fn from_preset(preset: Preset) -> Self {
        match preset {
            Preset::Collaborative => ConfidenceParams::default(),
            Preset::Authoritative => ConfidenceParams {
                w_base:  0.14, w_usage: 0.14, w_fresh: 0.10,
                w_help:  0.14, w_corr:  0.18, w_trust: 0.22,
                freshness_half_life_hours: 8760.0,
                alpha0: COLD_START_ALPHA,
                beta0:  COLD_START_BETA,
            },
            Preset::Operational => ConfidenceParams {
                w_base:  0.14, w_usage: 0.18, w_fresh: 0.24,
                w_help:  0.08, w_corr:  0.18, w_trust: 0.10,
                freshness_half_life_hours: 720.0,
                alpha0: COLD_START_ALPHA,
                beta0:  COLD_START_BETA,
            },
            Preset::Empirical => ConfidenceParams {
                w_base:  0.12, w_usage: 0.16, w_fresh: 0.34,
                w_help:  0.04, w_corr:  0.06, w_trust: 0.20,
                freshness_half_life_hours: 24.0,
                alpha0: COLD_START_ALPHA,
                beta0:  COLD_START_BETA,
            },
            Preset::Custom => {
                // Custom preset does not have built-in weights; caller must
                // use resolve_confidence_params() to apply [confidence] weights.
                // Calling from_preset(Custom) is a logic error.
                panic!("from_preset(Custom) is invalid; use resolve_confidence_params()");
            }
        }
    }
}
```

**Placement of `from_preset`**: This method is defined on `ConfidenceParams` in
`unimatrix-engine/src/confidence.rs`. However, `ConfidenceParams` in `unimatrix-engine`
must not depend on `Preset` (a server type). Therefore, `from_preset` is defined as a
free function in `unimatrix-server/src/infra/config.rs` that takes `Preset` and returns
`ConfidenceParams`, not as a method on `ConfidenceParams` itself:

```rust
// In unimatrix-server/src/infra/config.rs:
pub fn confidence_params_from_preset(preset: Preset) -> ConfidenceParams { ... }
```

The SR-10 test is also in `unimatrix-server`:

```rust
#[test]
fn collaborative_preset_equals_default() {
    assert_eq!(
        confidence_params_from_preset(Preset::Collaborative),
        ConfidenceParams::default()
    );
}
```

**Startup wiring:**

`main.rs` (`tokio_main_daemon` and `tokio_main_stdio`), after config load:

```rust
let config = load_config(home_dir, &paths.data_dir)?;
let confidence_params = resolve_confidence_params(&config)?;
// Pass confidence_params to the background tick and any other caller of
// compute_confidence(). Do not re-resolve on every tick call — the params
// are fixed at startup.
```

`confidence_params` is `Arc<ConfidenceParams>` if shared across tasks, or a plain
value stored in the background tick's spawn closure. It does NOT change at runtime
(config is loaded once).

**`[knowledge] freshness_half_life_hours` is the only operator-interpretable field:**

SCOPE.md: "This is the one tunable in the confidence system that any operator can
reason about without ML expertise." It belongs in `[knowledge]` because it is a
knowledge domain property (how quickly does knowledge in this domain become stale?),
not a weight property (how much does staleness factor into confidence scoring?).

### Decision

The full resolution pipeline is:

1. `load_config()` → `UnimatrixConfig` (ADR-003: two-level merge)
2. `validate_config()` — validates all fields including preset/weights combination
3. `resolve_confidence_params(&config)` → `Result<ConfidenceParams, ConfigError>`
   - Single resolution site for all confidence parameter sources
   - Named preset: use ADR-005 weight table + optional `[knowledge]` half_life override
   - `custom`: use `[confidence] weights` + required `[knowledge]` half_life
4. `Arc<ConfidenceParams>` passed to background tick and any other caller

The `[knowledge] freshness_half_life_hours` field in `KnowledgeConfig` uses `Option<f64>`:
- `None` (absent from TOML) → use preset's built-in half_life
- `Some(v)` → override preset's half_life with `v`

This `Option<f64>` avoids the false-positive problem in the merge logic: a `0.0`
default value would be indistinguishable from "absent" in the merge. The `Option` makes
presence vs. absence explicit at the type level.

The `validate_config` function contains the `custom` validation gate:

```rust
if config.profile.preset == Preset::Custom {
    match &config.confidence.weights {
        None => return Err(ConfigError::CustomPresetMissingWeights),
        Some(w) => {
            // validate each weight in [0.0, 1.0] and finite
            // validate sum == 0.92
        }
    }
    if config.knowledge.freshness_half_life_hours.is_none() {
        return Err(ConfigError::CustomPresetMissingHalfLife);
    }
}
```

### Consequences

**Easier:**
- SR-02 resolved: `resolve_confidence_params` always returns a fully-populated
  `ConfidenceParams` with all six weights, regardless of which preset is active.
- SR-11 resolved: single resolution site — exactly one place determines which
  `freshness_half_life_hours` value is used.
- SR-13 resolved: W3-1 reads `ConfidenceParams` from the server's startup state;
  the six weights are there from dsn-001 forward.
- The `Option<f64>` for `freshness_half_life_hours` makes "not specified" explicit at
  the type level — no ambiguous zero-vs-absent issue in merge logic.
- `validate_config` catches all `custom` configuration errors at startup rather than
  at first confidence computation call.

**Harder:**
- `from_preset(Custom)` panics by design — calling it directly is a logic error. Code
  review must ensure only `resolve_confidence_params` is called for `custom` paths.
- `ConfidenceParams` is defined in `unimatrix-engine` but `from_preset`-equivalent is in
  `unimatrix-server` — the preset-to-params table is split across two crates. The weight
  values in `confidence_params_from_preset` must be kept in sync with ADR-005 and with
  `ConfidenceParams::default()`. The SR-10 test is the mechanical guard against drift.
- Operators who set `preset = "custom"` must specify both `[confidence] weights` AND
  `[knowledge] freshness_half_life_hours`. Forgetting either causes startup abort. The
  error message must clearly identify which field is missing.
