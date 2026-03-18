## ADR-004: [confidence] Section — Promoted from Stub to Live (custom preset only)

### Context

The first design pass of dsn-001 (Unimatrix #2287) designated `ConfidenceConfig` as an
empty forward-compatibility stub: no fields active, reserved for W3-1 GNN cold-start.

That design has been superseded by the preset system. The `[confidence]` section is now
a live section — active when `[profile] preset = "custom"`. It is NOT a stub.

The change: SCOPE.md (updated) defines `[confidence] weights` as the expert escape
hatch for operators who have domain-science justification for specific weight values:

```toml
[profile]
preset = "custom"

[confidence]
weights = { base = 0.16, usage = 0.16, fresh = 0.18, help = 0.12, corr = 0.14, trust = 0.16 }
```

When `preset != "custom"`, the `[confidence]` section is IGNORED entirely, even if
present in the config file. Named presets use their built-in weight tables (see
ADR-005); `[confidence]` values have no effect.

When `preset == "custom"`, all six weights are REQUIRED. Startup aborts with a
descriptive error if any are absent (AC-24).

The `[cycle]` stub from the original ADR-004 is unchanged: the `CycleConfig` struct
remains an empty reserved namespace. The doc fix for `context_retrospective` →
`context_cycle_review` and `CycleParams.topic` is hardcoded, not runtime config.

**SR-12 risk resolved**: "The forward-compat stub for `[confidence]` was designed
before the preset expansion. Now `[confidence] weights` is an active, live section
for `custom` preset — not a stub. The stub design assumed the section would be
ignored until W3-1. If the `ConfidenceConfig` struct was scaffolded as an empty stub,
it must now be promoted to a real struct with validation before delivery."

This ADR records that promotion.

**W3-1 relationship**: W3-1 (GNN Confidence Learning) cold-starts from the active
preset's weight vector, not from `[confidence] weights`. The product vision states:
"Cold-start from config-defined weights — W3-1 initializes from the weights in
`[confidence] weights` config." In the preset system, this means W3-1 reads
`ConfidenceParams` (which carries the resolved weight vector regardless of whether
it came from a named preset or `custom`). W3-1 does not need to read `[confidence]`
directly — it reads the resolved `ConfidenceParams` that the server already holds.
W3-1 may add a `learned_weights` field to `ConfidenceParams` later (per ADR-001).

### Decision

`ConfidenceConfig` is promoted from an empty stub to a real struct with one field:

```rust
/// Active only when `[profile] preset = "custom"`.
/// All six weight fields are required when active.
/// Ignored entirely for named presets (authoritative, operational,
/// empirical, collaborative).
#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct ConfidenceConfig {
    /// Expert escape hatch: raw weight values.
    /// Only used when preset = "custom". See ADR-005 for weight constraints.
    pub weights: Option<ConfidenceWeights>,
}

/// Six-component weight vector for confidence scoring.
/// All six are required when present; each in [0.0, 1.0]; sum must be 0.92.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct ConfidenceWeights {
    pub base:  f64,
    pub usage: f64,
    pub fresh: f64,
    pub help:  f64,
    pub corr:  f64,
    pub trust: f64,
}
```

Validation rules (enforced in `validate_config`, only when `preset == "custom"`):
- `weights` must be `Some` — all six are required (AC-24).
- Each weight in `[0.0, 1.0]` and finite.
- Sum must equal `0.92` within floating-point tolerance (`(sum - 0.92).abs() < 1e-9`).
- Startup aborts with a descriptive error on any violation.

When `preset != "custom"`:
- `weights` is not validated.
- `weights` is not read during `ConfidenceParams` construction.
- Operator-supplied `[confidence]` values have zero effect on scoring.

`CycleConfig` remains an empty reserved namespace stub (no fields, no change from the
original ADR-004):

```rust
/// Reserved for future domain-label customisation.
/// No fields are active in dsn-001.
#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct CycleConfig {}
```

`UnimatrixConfig` now has five sections: `profile`, `knowledge`, `server`, `agents`,
`confidence`. The `cycle` field is removed from `UnimatrixConfig` — it was never
active and the vocabulary fix it reserved for is now a hardcoded rename, not config.
If a future feature needs `[cycle]` config, the field can be added then.

### Consequences

**Easier:**
- `[confidence] weights` is fully implemented with validation — SR-12 is resolved.
- Operators using named presets cannot accidentally activate `[confidence]` — the
  section is silently ignored unless `preset = "custom"`.
- W3-1 cold-start reads `ConfidenceParams` (already resolved by the server at
  startup) rather than re-parsing config — no new W3-1 config format needed.
- `CycleConfig` removal simplifies `UnimatrixConfig` by one unused field.

**Harder:**
- The delivery team must not treat `ConfidenceConfig` as a stub. It requires full
  validation and test coverage (AC-24, AC-25 from SCOPE.md).
- Operators who set `[confidence] weights` with a named preset will see their
  values silently ignored. The documentation must make this explicit.
- The weight sum constraint `== 0.92` (not `≤ 1.0` as the SCOPE.md config schema
  comment says) is the actual invariant — the SCOPE comment is incorrect and must
  not be used as the validation rule. See ADR-005 for the authoritative constraint.
