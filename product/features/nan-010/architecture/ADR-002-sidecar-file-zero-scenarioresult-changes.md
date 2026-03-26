## ADR-002: Sidecar File for Profile Metadata — Zero Changes to ScenarioResult

### Context

`eval report` requires the `distribution_change` flag and `DistributionTargets` from each
profile's TOML to determine whether Section 5 shows a Distribution Gate or a Zero-Regression
Check. `eval report` reads only per-scenario JSON files from the results directory; it does not
have access to the original TOML paths.

Two approaches were evaluated in the scope:
1. Add `profile_metadata: HashMap<String, ProfileMeta>` to `ScenarioResult` — embedded in every
   result file.
2. Write a separate `profile-meta.json` sidecar to the output directory during `eval run`,
   which `eval report` reads alongside the result files.

Option 1 triggers the dual-type constraint (pattern #3574, documented in Unimatrix entries
#3574, #3550, #3512): `runner/output.rs` and `report/mod.rs` maintain independent copies of
all result types (`ScenarioResult`, `ProfileResult`, `ComparisonMetrics`, `RankChange`,
`ScoredEntry`). Adding a field to `ScenarioResult` requires a three-site update: runner copy,
report copy, and round-trip integration tests. This constraint caused rework in nan-007,
nan-008, and nan-009. Option 1 also embeds per-run metadata into per-scenario files, mixing
two levels of granularity.

Option 2 — the sidecar — keeps metadata at the run level where it belongs. The results
directory is the artifact boundary: everything needed to reproduce the report must live there.
`eval report` can be re-run post-hoc from a CI artifact directory without the original TOMLs.

`profile-meta.json` schema (version 1):

```json
{
  "version": 1,
  "profiles": {
    "<profile-name>": {
      "distribution_change": false,
      "distribution_targets": null
    },
    "<candidate-name>": {
      "distribution_change": true,
      "distribution_targets": {
        "cc_at_k_min": 0.60,
        "icd_min": 1.20,
        "mrr_floor": 0.35
      }
    }
  }
}
```

The `"version": 1` field is included from the start (design decision #2 in SCOPE.md §Design
Decisions). It costs nothing now and avoids "absent field = version 1" inference later.

When `profile-meta.json` is absent from the results directory (`eval report` run against
pre-nan-010 results), `run_report` returns an empty `HashMap` and treats all profiles as
`distribution_change = false`. This is full backward compatibility (AC-11).

When `profile-meta.json` is present but malformed, `run_report` logs a WARN and falls back to
empty map — the same backward-compat behavior. A corrupt sidecar must never fail `eval report`
hard (invariant: `run_report` always returns `Ok(())`).

### Decision

Profile metadata is carried via a sidecar `profile-meta.json` file written to the output
directory by `eval run`. `eval report` reads it as an optional artifact.

**Zero changes are made to `ScenarioResult` fields in either `runner/output.rs` or
`report/mod.rs`**. This is a hard implementation constraint. If any future design change
requires touching `ScenarioResult`, the dual-type three-site sync protocol must be followed
(runner + report + round-trip tests).

The sidecar uses serde-enabled types (`ProfileMetaFile`, `ProfileMetaEntry`,
`DistributionTargetsJson`) that are distinct from the in-memory `EvalProfile` /
`DistributionTargets` types. The JSON types live in `eval/runner/profile_meta.rs`.

### Consequences

Easier:
- No change to the dual-type copies; no risk of the three-site sync being missed.
- `eval report` is self-contained from the results directory; works post-hoc on CI artifacts.
- Profile metadata and per-scenario results remain at the correct granularity levels.
- `"version": 1` enables future schema evolution without inference games.

Harder:
- `eval run` now has an additional output artifact; partial runs (crash after result files,
  before sidecar flush) leave the sidecar absent. This is mitigated by ADR-004 (atomic write)
  and the backward-compat fallback in `run_report`.
- Documentation must explain the sidecar contract (what "absent file" means).
