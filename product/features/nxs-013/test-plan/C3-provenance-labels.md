# Test Plan: C3 -- log_config_provenance Labels

## Component

Update log message strings in `log_config_provenance` (main.rs, lines 1347-1375). Per-project becomes "primary (per-project)", global becomes "defaults (global)".

## Risks Covered

- **R-02** (Med): Log label change alters control flow
- **R-03** (Med): Log label changes untestable via automation

## Unit Test Expectations

### Existing Tests (Must Pass Unchanged)

The 7 provenance tests in config.rs (lines 9219-9340) assert on `SourceStatus` enum variants:
- `SourceStatus::Loaded { path }` -- asserts the path value, not any log string
- `SourceStatus::NotFound { path }` -- asserts the path value
- `SourceStatus::NotApplicable` -- asserts the variant

These tests do NOT call `log_config_provenance`. They test `load_config` which returns `ConfigProvenance`. The log function consumes the provenance struct separately.

**Expected result**: All 7 provenance tests pass. All 4 category authority tests pass. Zero test file changes.

### Why No New Unit Tests

`log_config_provenance` produces `tracing` output via `info!()`, `debug!()`, and `warn!()` macros. Testing tracing output requires a tracing-test subscriber harness. This is not in scope for nxs-013 (cosmetic label change, not behavioral). R-03 explicitly acknowledges this: verification is via code review + manual log inspection.

## Code Review Checklist

- [ ] `log_config_provenance` function signature unchanged
- [ ] Match arms match on the same `SourceStatus` variants as before (`Loaded`, `NotFound`, `NotApplicable`)
- [ ] Log levels unchanged: `info!` for loaded, `debug!` for not-found, `warn!` for applicable warnings
- [ ] Only string literals inside macro calls changed
- [ ] No control flow modifications (no new `if`, `match`, `return`, `break`)
- [ ] No changes to the `env_override` branch labels

## String Literal Assertions

Verify exact new labels in source:

| Branch | Variant | Expected String Contains |
|--------|---------|-------------------------|
| `provenance.global` | `Loaded` | `"defaults config loaded (global)"` |
| `provenance.global` | `NotFound` | `"defaults config not found (global); using compiled defaults"` |
| `provenance.project` | `Loaded` | `"primary config loaded (per-project)"` |
| `provenance.project` | `NotFound` | `"primary config not found (per-project); write default with 'unimatrix config'"` |
| `provenance.env_override` | any | **Unchanged** -- no edits to this branch |

## Manual Verification (AC-03, AC-10)

### MV-01: Both Config Files Present
- **Arrange**: Run daemon with both global and per-project config files present
- **Act**: Inspect startup log output
- **Assert**: Log contains "primary config loaded (per-project)" and "defaults config loaded (global)"

### MV-02: Neither Config File Present
- **Arrange**: Run daemon with no config files (empty data dir, no global config)
- **Act**: Inspect startup log output
- **Assert**: Log contains "primary config not found (per-project)" and "defaults config not found (global)"

### MV-03: Only Per-Project Config
- **Arrange**: Run daemon with per-project config only
- **Act**: Inspect startup log output
- **Assert**: Log contains "primary config loaded (per-project)" and "defaults config not found (global)"

## Edge Cases

- **Labels swapped**: If "primary" appears in the global branch and "defaults" in the per-project branch, operators get confused. Code review + MV-01 catch this.
- **Partial edit**: If only one branch is updated and the other retains old labels ("global config loaded" alongside "primary config loaded"), inconsistency is visible in MV-01. Code review catches partial edits.
