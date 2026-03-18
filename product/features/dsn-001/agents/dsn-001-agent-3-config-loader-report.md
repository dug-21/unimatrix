# Agent Report: dsn-001-agent-3-config-loader

**Agent ID**: dsn-001-agent-3-config-loader
**Component**: config-loader (`crates/unimatrix-server/src/infra/config.rs`)
**Feature**: dsn-001 — Config Externalization (W0-3)
**GH Issue**: #306

---

## Files Created / Modified

| File | Action |
|------|--------|
| `crates/unimatrix-server/src/infra/config.rs` | CREATED (488 lines implementation + 729 lines tests) |
| `crates/unimatrix-server/Cargo.toml` | MODIFIED — added `toml = "0.8"` |
| `crates/unimatrix-server/src/infra/mod.rs` | MODIFIED — added `pub mod config;` |

---

## What Was Implemented

All items from the spawn prompt:

- `UnimatrixConfig` struct with 5 sub-structs: `ProfileConfig`, `KnowledgeConfig`, `ServerConfig`, `AgentsConfig`, `ConfidenceConfig`
- `ConfidenceWeights` struct (no `Default` — prevents silent zero-initialization)
- `Preset` enum with `#[serde(rename_all = "lowercase")]`, `#[default]` on `Collaborative`
- `ConfigError` enum — all 17 variants with `Display` including path, field, and valid range
- `load_config(home_dir, data_dir)` — two-level merge with ContentScanner ordering invariant
- `validate_config(config, path)` — independently testable (no tokio, no store)
- `resolve_confidence_params(config)` — single ADR-006 resolution site
- `confidence_params_from_preset(preset)` — panics on `Custom` by design
- `merge_configs(global, project)` — replace semantics, `Option::or` for optional fields
- `check_permissions(path)` — `#[cfg(unix)]`, uses `metadata()` (not `symlink_metadata()`)

### Critical invariants implemented

- Weight sum: `(sum - 0.92).abs() >= 1e-9` aborts (NOT `sum <= 1.0`)
- ContentScanner warmed at top of `load_config` with comment documenting the ordering invariant
- `KnowledgeConfig.freshness_half_life_hours` is `Option<f64>` (None = absent, not default)
- Cross-level custom preset inheritance: enforced at per-file validation before merge
- `dirs::home_dir() = None`: handled by caller passing non-existent path → both files absent → defaults

---

## Test Results

```
test result: ok. 72 passed; 0 failed; 0 ignored
```

### Test coverage

- All 17 `ConfigError` variants (Display coverage)
- All 4 AC-25 freshness precedence cases (named tests)
- SR-10 mandatory test (collaborative = default)
- Weight sum invariant: 0.95 rejected, 0.92 passes, both sides of 1e-9 boundary
- Named preset immunity to `[confidence]` weights
- Cross-level custom preset prohibition
- Size cap at 64 KB boundary (65536 pass, 65537 fail)
- Unix permission tests: world-writable abort, group-writable warn, symlink follow
- Merge replace semantics, list replace not append
- Malformed TOML wrapped as `MalformedToml`
- Unrecognised preset fails serde before `validate_config`
- Empty file produces defaults
- Instructions length-before-scan ordering (injection padded to 9003 bytes → `InstructionsTooLong`)
- `[agents]` Admin exclusion (uppercase and lowercase)

### Pre-existing failures (not introduced by this agent)

10 pre-existing failures in `import::tests` and `mcp::identity::tests` (pool timeout, GH#303) — confirmed by git stash comparison.

---

## Issues / Blockers

None. The `ConfidenceParams` struct (9 fields) was already added to `unimatrix-engine/src/confidence.rs` by the confidence-params agent before this agent ran. No deviations from pseudocode.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found #2298 (TOML key semantic divergence) and #646 (serde(default) backward-compatible extension). Both applied.
- Stored: entry #2312 "config.rs: boosted_categories default causes empty-categories validation to fail" via `/uni-store-pattern`
- Stored: entry #2313 "config.rs: cross-level custom preset prohibition enforced at per-file validation, not at merge" via `/uni-store-pattern`

### Discovery: boosted_categories default breaks empty-categories test

The test plan's `test_empty_categories_documented_behavior` assumes an empty categories list passes validation. It does not — because the default `boosted_categories = ["lesson-learned"]` is cross-validated against the categories list. The test must explicitly set both to empty. Fixed in implementation with a clear comment.

### Discovery: merge Order::or for confidence.weights means merged config may have global weights

When `global.confidence.weights = Some(...)` and `project.confidence.weights = None`, the `Option::or` in `merge_configs` fills in global's weights. The ADR-003 prohibition ("per-project custom with no per-project weights aborts") is only enforced at per-file validation time, not at merged validation time. Tests that test the merged result expecting failure will incorrectly pass. Fixed the test to validate the per-project file isolation, matching the pseudocode's documented intent.
