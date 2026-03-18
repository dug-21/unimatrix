# Agent Report: dsn-001-agent-1-architect (REVISED — Preset System)

## Status: COMPLETE

## Artifacts Produced

- `/workspaces/unimatrix/product/features/dsn-001/architecture/ARCHITECTURE.md` (overwritten)
- `/workspaces/unimatrix/product/features/dsn-001/architecture/ADR-001-confidence-params-struct.md` (extended — Unimatrix #2284, needs context_correct update)
- `/workspaces/unimatrix/product/features/dsn-001/architecture/ADR-002-config-type-placement.md` (updated — Unimatrix #2285, needs context_correct update)
- `/workspaces/unimatrix/product/features/dsn-001/architecture/ADR-003-two-level-config-merge.md` (updated — Unimatrix #2286, needs context_correct update)
- `/workspaces/unimatrix/product/features/dsn-001/architecture/ADR-004-forward-compat-stubs.md` (updated — Unimatrix #2287, needs context_correct update — stub promoted to live)
- `/workspaces/unimatrix/product/features/dsn-001/architecture/ADR-005-preset-enum-and-weights.md` (new — needs context_store, Unimatrix ID TBD)
- `/workspaces/unimatrix/product/features/dsn-001/architecture/ADR-006-preset-resolution-pipeline.md` (new — needs context_store, Unimatrix ID TBD)

## Unimatrix Storage Required

ADR-001 through ADR-004 (IDs #2284–#2287) must be updated via `context_correct`.
ADR-005 and ADR-006 must be created via `context_store`.
The Design Leader must complete these Unimatrix operations before closing the design session.

## Key Decisions

1. **ADR-001 (extended)**: `ConfidenceParams` gains six weight fields (w_base,
   w_usage, w_fresh, w_help, w_corr, w_trust). `Default` reproduces compiled
   constants exactly. `compute_confidence` uses `params.w_*`. SR-02 resolved.

2. **ADR-002 (updated)**: `UnimatrixConfig` in `unimatrix-server`. `Preset` enum
   and `confidence_params_from_preset` also in `unimatrix-server/src/infra/config.rs`.
   `ConfidenceParams` crosses to engine as a value, not `Arc<Config>`.

3. **ADR-003 (updated)**: `custom` preset with no per-project `[confidence] weights`
   does NOT inherit global weights — each level is self-contained.

4. **ADR-004 (promoted)**: `ConfidenceConfig` is now a live struct with
   `weights: Option<ConfidenceWeights>`. Active only for `preset = "custom"`.
   `CycleConfig` stub removed from `UnimatrixConfig`.

5. **ADR-005 (new)**: Preset enum and exact weight table (all rows verified = 0.92).
   Ordering relationships validated against domain archetypes.

6. **ADR-006 (new)**: Single resolution site `resolve_confidence_params()`. Explicit
   `freshness_half_life_hours` precedence chain for all four preset/override combinations.

## Exact Preset Weight Table (ADR-005)

| Preset | w_base | w_usage | w_fresh | w_help | w_corr | w_trust | SUM  | half_life |
|--------|--------|---------|---------|--------|--------|---------|------|-----------|
| `collaborative` | 0.16 | 0.16 | 0.18 | 0.12 | 0.14 | 0.16 | **0.92** | 168.0h |
| `authoritative` | 0.14 | 0.14 | 0.10 | 0.14 | 0.18 | 0.22 | **0.92** | 8760.0h |
| `operational`   | 0.14 | 0.18 | 0.24 | 0.08 | 0.18 | 0.10 | **0.92** | 720.0h |
| `empirical`     | 0.12 | 0.16 | 0.34 | 0.04 | 0.06 | 0.20 | **0.92** | 24.0h |

All four sums verified to 0.92 exactly (Python IEEE 754 verification).

## Critical Corrections for Delivery Team

- Weight sum validation: use `(sum - 0.92).abs() < 1e-9`, NOT `sum <= 1.0`
  (SCOPE.md config schema comment is wrong on this point)
- `confidence_params_from_preset(Preset::Custom)` panics — logic error, only
  `resolve_confidence_params(&config)` handles Custom
- SR-10 test is mandatory before PR opens

## ADR File Paths (for synthesizer)

- `product/features/dsn-001/architecture/ADR-001-confidence-params-struct.md` — #2284 (updated)
- `product/features/dsn-001/architecture/ADR-002-config-type-placement.md` — #2285 (updated)
- `product/features/dsn-001/architecture/ADR-003-two-level-config-merge.md` — #2286 (updated)
- `product/features/dsn-001/architecture/ADR-004-forward-compat-stubs.md` — #2287 (updated)
- `product/features/dsn-001/architecture/ADR-005-preset-enum-and-weights.md` — TBD (new)
- `product/features/dsn-001/architecture/ADR-006-preset-resolution-pipeline.md` — TBD (new)
