# nan-010 Researcher Agent Report

## Summary

SCOPE.md written to `product/features/nan-010/SCOPE.md`.

The scope is well-bounded. The core behavioral change is entirely in the reporting layer
(`eval/report/`). The parse layer (`eval/profile/`) needs modest extension. The runner
needs to emit a side-car file. No changes to replay logic, metrics computation, or any
other subsystem.

## Key Findings

### Files Directly Affected

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/eval/profile/types.rs` | Add `DistributionTargets` struct; extend `EvalProfile` with `distribution_change: bool` and `distribution_targets: Option<DistributionTargets>` |
| `crates/unimatrix-server/src/eval/profile/validation.rs` | Extend `parse_profile_toml` to extract and validate new `[profile]` fields before stripping the section |
| `crates/unimatrix-server/src/eval/runner/mod.rs` | After layer construction in `run_eval_async`, write `profile-meta.json` to `--out` directory |
| `crates/unimatrix-server/src/eval/report/aggregate.rs` | Add `check_distribution_targets`; verify 500-line limit (488 lines currently) |
| `crates/unimatrix-server/src/eval/report/render.rs` | Conditional Section 5 dispatch; currently at 499 lines — must not grow here |
| `crates/unimatrix-server/src/eval/report/render_distribution_gate.rs` | New file: `render_distribution_gate_section` |
| `crates/unimatrix-server/src/eval/report/mod.rs` | Load `profile-meta.json`; pass metadata to `render_report` |
| `docs/testing/eval-harness.md` | Document new flag, sub-table, Distribution Gate section, and example TOML |

### Files NOT Affected

- `eval/runner/metrics.rs` — CC@k and ICD are already computed for every profile
- `eval/runner/replay.rs` — replay logic is unchanged
- `eval/runner/output.rs` — ScenarioResult is unchanged (metadata goes in side-car)
- `eval/report/mod.rs` local `ScenarioResult` copy — unchanged (dual-type constraint not triggered)
- `eval/scenarios/` — unchanged
- `eval/profile/layer.rs` — unchanged

### Critical TOML Parsing Constraint

`parse_profile_toml` strips the `[profile]` section before deserializing `UnimatrixConfig`
(validation.rs lines 85–102). `distribution_change` and `distribution_targets` are nested
under `[profile]`, so they must be extracted from the raw `toml::Value` *before* the strip,
following the existing `name`/`description` extraction pattern at lines 66–80.

### render.rs Is at the 500-Line Limit

`render.rs` is exactly 499 lines as of nan-009. Any distribution gate rendering logic must
go in a new `render_distribution_gate.rs` sibling module. This is a hard constraint from
the workspace rules.

### aggregate.rs Line Count

`aggregate.rs` is 488 lines. The new `check_distribution_targets` function (estimated
~40–60 lines including tests) will exceed 500 lines if placed here. It should go in a new
`aggregate_distribution.rs` sub-module, or the function must be placed in
`render_distribution_gate.rs` if it is small enough to be co-located with the renderer.

### No Changes to Dual-Type Constraint

The profile metadata is carried via `profile-meta.json` rather than being added to
`ScenarioResult`. This means the dual-type constraint (pattern #3574) is not triggered.
`runner/output.rs` and `report/mod.rs` local copies of `ScenarioResult` remain unchanged.

### Backward Compatibility Is the Default

When `profile-meta.json` is absent from the results directory, `eval report` must fall
back to treating all profiles as `distribution_change = false` — the zero-regression check
runs as before. This backward-compat path must be explicitly tested (AC-14).

## Open Questions Requiring Human Decision

1. **Multi-profile mixed mode** — when some candidates declare `distribution_change = true`
   and others do not, should Section 5 be per-profile (recommended) or a single global mode?

2. **profile-meta.json versioning** — include a `"version": 1` field from the start?
   (Recommended yes.)

3. **`mrr_floor` semantics** — is it a veto (always checked independently) or just a third
   must-pass target? The issue description says "OR MRR drops below the absolute floor"
   which is semantically equivalent to a third must-pass target. Confirm before spec.

4. **Where `run_report` loads profile-meta.json** — self-loading from the results directory
   (option a, recommended) or passed in from the dispatch caller (option b)?

## Risks

- `render.rs` line limit is a hard wall. The split into `render_distribution_gate.rs` is
  mandatory, not optional.
- `aggregate.rs` is close to the 500-line limit. `check_distribution_targets` may force
  a second aggregate module split.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "eval harness profile TOML evaluation" (pattern
  category) — found patterns #3574 (dual-type constraint) and #3526 (round-trip test
  strategy), both relevant and current.
- Queried: `/uni-query-patterns` for "eval harness CC@k ICD distribution metrics"
  (convention category) — no relevant eval harness conventions found.
- Queried: `/uni-query-patterns` for "eval harness render section gating pass fail report"
  (pattern category) — no existing pattern for conditional gate-section rendering.
- Stored: entry #3582 "Eval Harness: Side-Car Metadata File Pattern for Per-Profile
  Run-Time Flags" via `/uni-store-pattern`.
- Stored: entry #3583 "Eval Harness render.rs 500-Line Split: New Section = New
  render_*.rs Module" via `/uni-store-pattern`.
