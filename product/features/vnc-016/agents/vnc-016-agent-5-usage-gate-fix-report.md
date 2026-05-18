# Agent Report: vnc-016-agent-5-usage-gate-fix

## Task

Implement the usage gate fix component: add `write_capable: bool` to `UsageContext`,
replace the trust-level gate in both `record_mcp_usage` and `record_hook_injection`,
and set `write_capable` at all `UsageContext` construction sites in `tools.rs`.

## Files Modified

- `crates/unimatrix-server/src/services/usage.rs`
- `crates/unimatrix-server/src/mcp/tools.rs`

## Changes Made

### `usage.rs`

- Added `pub write_capable: bool` as the last field in `UsageContext` (line 79). No
  `Default` derivation, no `#[serde(default)]` — exhaustive struct construction
  enforces explicit setting at every callsite (C-11).
- Replaced trust-level match in `record_mcp_usage` gate block (was lines 207-218)
  with `if ctx.write_capable` (C-12).
- Replaced identical trust-level match in `record_hook_injection` gate block (was
  lines 272-283) with `if ctx.write_capable` (C-12).
- Added `write_capable` to all existing `UsageContext` literals in the test module:
  - `write_capable: true` on tests that assert feature_entries ARE written
    (`test_record_access_mcp_feature_recording`,
    `test_usage_context_current_phase_propagates_to_feature_entry`,
    `test_usage_context_phase_none_produces_null_phase`).
  - `write_capable: false` on all other test literals (no feature_entries expected).
- Added two new unit tests (AC-13):
  - `test_write_capable_false_yields_no_feature_recording` — gate yields `None`
  - `test_write_capable_true_yields_feature_recording` — gate yields `Some(...)`

### `tools.rs`

- `context_search` handler (`UsageContext` ~line 481): `write_capable: false`
- `context_lookup` handler (`UsageContext` ~line 618): `write_capable: false`
- `context_store` handler (`UsageContext` ~line 836, inside `if let Some(fc)`
  branch): `write_capable: true` — `require_cap(Write)` has already passed (C-13)
- `context_get` handler (`UsageContext` ~line 933): `write_capable: false`
- `context_briefing` handler (`UsageContext` ~line 1606): `write_capable: false`

## Tests

```
test services::usage::usage_tests::test_write_capable_false_yields_no_feature_recording ... ok
test services::usage::usage_tests::test_write_capable_true_yields_feature_recording ... ok
```

2 passed, 0 failed.

Full suite: `cargo test --workspace` — all test results `ok`, 0 failures across all crates.

## Constraint Coverage

| Constraint | Status |
|-----------|--------|
| C-11 (no Default, no serde default) | PASS — compile enforces every construction site |
| C-12 (both gate blocks fixed) | PASS — record_mcp_usage + record_hook_injection both replaced |
| C-13 (write_capable: true unconditional at context_store) | PASS — set inside if let Some(fc) branch |
| NFR-08 (trust_level retained on UsageContext) | PASS — field unchanged, still set at all sites |

## Issues

None. Build clean (`cargo build --workspace` exits 0). Pre-existing clippy warnings
in `unimatrix-engine` are unrelated to this component.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #4451 (ADR-002,
  the architect's decision) and entry #4450 (ADR-007 predecessor). Applied directly.
- Stored: entry #4453 "Propagate capability checks into service context via dedicated
  bool — do not use trust level as proxy" via `/uni-store-pattern`.
