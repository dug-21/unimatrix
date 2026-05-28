# C3: log_config_provenance Labels

## Purpose

Update log message strings in `log_config_provenance` to reflect the canonical hierarchy: per-project is "primary", global is "defaults". Per ADR-002, no summary line is added.

## Target File

`crates/unimatrix-server/src/main.rs`, function `log_config_provenance` at lines 1347-1375.

## Current State

```rust
fn log_config_provenance(provenance: &ConfigProvenance) {
    match &provenance.global {
        SourceStatus::Loaded { path } => {
            tracing::info!(path = %path.display(), "global config loaded");
        }
        SourceStatus::NotFound { path } => {
            tracing::info!(path = %path.display(), "global config not found; using compiled defaults");
        }
        SourceStatus::NotApplicable => {}
    }
    match &provenance.project {
        SourceStatus::Loaded { path } => {
            tracing::info!(path = %path.display(), "project config loaded");
        }
        SourceStatus::NotFound { path } => {
            tracing::warn!(path = %path.display(), "project config not found; using compiled defaults");
        }
        SourceStatus::NotApplicable => {}
    }
    match &provenance.env_override {
        SourceStatus::Loaded { path } => {
            tracing::info!(path = %path.display(), "env override config loaded (UNIMATRIX_CONFIG)");
        }
        SourceStatus::NotFound { path } => {
            tracing::warn!(path = %path.display(), "UNIMATRIX_CONFIG set but file not found");
        }
        SourceStatus::NotApplicable => {}
    }
}
```

## Pseudocode

```
CHANGE ONLY the string literal arguments to the tracing macros. 
DO NOT change match arms, log levels, control flow, or the env_override branch.

Label mapping (4 string replacements):

1. global / Loaded:
   OLD: "global config loaded"
   NEW: "defaults config loaded (global)"

2. global / NotFound:
   OLD: "global config not found; using compiled defaults"
   NEW: "defaults config not found (global); using compiled defaults"

3. project / Loaded:
   OLD: "project config loaded"
   NEW: "primary config loaded (per-project)"

4. project / NotFound:
   OLD: "project config not found; using compiled defaults"
   NEW: "primary config not found (per-project); write default with 'unimatrix config'"

env_override branch: NO CHANGES (already correct).
```

## What Does NOT Change

- Function signature: `fn log_config_provenance(provenance: &ConfigProvenance)` -- unchanged.
- Match arm patterns: `SourceStatus::Loaded`, `SourceStatus::NotFound`, `SourceStatus::NotApplicable` -- unchanged.
- Log levels: `tracing::info!` for global/Loaded, `tracing::info!` for global/NotFound, `tracing::info!` for project/Loaded, `tracing::warn!` for project/NotFound -- unchanged.
- The `env_override` match block -- unchanged entirely.
- The `path = %path.display()` field in each tracing macro -- unchanged.

## Constraints

- C-01: `load_config` is NOT modified.
- C-05: `ConfigProvenance` and `SourceStatus` types are NOT modified.
- NFR-02: Existing tests pass unmodified (tests assert on types, not log strings -- SR-06).

## Error Handling

Not applicable (string literal changes only). The function does not return errors.

## Key Test Scenarios

1. `cargo test --workspace` passes with zero test file changes -- confirms `SourceStatus` matching unaffected.
2. Code review: only 4 string literals changed inside `tracing::info!`/`tracing::warn!` macros.
3. Code review: match arm patterns identical to current code.
4. Code review: log levels identical to current code.
5. Manual: start daemon with both config files present -- logs show "primary" and "defaults" labels.
6. Manual: start daemon with neither config file -- fallback messages use new labels.
