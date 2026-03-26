# Component 7: Report Sidecar Load

**File**: `eval/report/mod.rs`
**Action**: Modify

---

## Purpose

Extend `run_report` to load `profile-meta.json` from the results directory between the
existing aggregation steps, compute per-profile distribution gate results, and thread both
maps into `render_report`. Also adds new module declarations and imports.

This component wires all other components together at the report entry point.

---

## New Module Declarations in `report/mod.rs`

Add to the existing `mod` block:
```
mod render_distribution_gate;
// Note: render.rs already declares this; mod.rs needs it only if the module
// is in report/ and re-exported. Actually the module declaration lives in render.rs.
// CLARIFICATION: mod declarations for submodules of report/ belong in mod.rs,
// not in render.rs. render.rs only adds `mod render_distribution_gate;` if
// render_distribution_gate.rs is a sibling of render.rs.
```

IMPORTANT: In the existing pattern, all `mod` declarations for report submodules live in
`report/mod.rs`. The `render_phase.rs` module is declared in `mod.rs` as:
```
mod render_phase;
```
...and `render.rs` imports it with:
```
use super::render_phase::render_phase_section;
```

For consistency, `render_distribution_gate.rs` should be declared in `mod.rs` as:
```
mod render_distribution_gate;
```
And `render.rs` imports it with:
```
use super::render_distribution_gate::{render_distribution_gate_section, HeadingLevel};
```

Verify against the existing `render_phase` pattern before implementation.

---

## New `#[cfg(test)]` Declaration

```
#[cfg(test)]
mod tests_distribution_gate;
```

Added alongside the existing test module declarations.

---

## New Imports in `report/mod.rs`

```
use crate::eval::runner::profile_meta::{ProfileMetaEntry, ProfileMetaFile, DistributionTargetsJson};
use crate::eval::profile::DistributionTargets;
use aggregate::distribution::{check_distribution_targets, DistributionGateResult};
```

---

## New Private Function: `load_profile_meta`

```
fn load_profile_meta(
    dir: &Path,
) -> Result<HashMap<String, ProfileMetaEntry>, Box<dyn std::error::Error>>

    path = dir.join("profile-meta.json")

    // Absent file → backward compat (AC-11, ADR-002, FR-09)
    if !path.exists():
        return Ok(HashMap::new())

    // File exists — attempt to read and parse
    content = fs::read_to_string(&path)?
    // If read fails (permission denied, etc.), propagate as error.
    // This is different from absent — a present-but-unreadable file is an error.

    // Attempt JSON deserialization
    match serde_json::from_str::<ProfileMetaFile>(&content):
        Ok(meta_file) =>
            // Return the profiles map from the sidecar
            Ok(meta_file.profiles)

        Err(parse_err) =>
            // Corrupt sidecar → abort with clear error message (ADR-002 updated,
            // ARCHITECTURE.md Component 7, RISK-TEST-STRATEGY R-07 resolution)
            // Do NOT silently fall back to empty map.
            Err(format!(
                "profile-meta.json is malformed — re-run eval to regenerate (parse error: {parse_err})"
            ).into())
```

Note: The return type uses `Box<dyn std::error::Error>` to match `run_report`'s return type
and allow the `?` operator. The error message satisfies the R-07 requirement: "profile-meta.json
is malformed — re-run eval to regenerate".

---

## Modified Function: `run_report`

Show only the new steps inserted into the existing function body:

```
pub fn run_report(
    results: &Path,
    scenarios: Option<&Path>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>

    // Step 1: Enumerate result JSON files (unchanged)
    // Step 2: Deserialize ScenarioResults (unchanged)
    // Step 3: Load scenario query map (unchanged)

    // === Step 3.5 (NEW): Load profile-meta.json sidecar ===
    let profile_meta: HashMap<String, ProfileMetaEntry> =
        load_profile_meta(results)?
        // If Err: propagate immediately — report aborts, exits non-zero.
        // This satisfies the corrupt-sidecar abort requirement (R-07, ADR-002 updated).

    // Step 4: Aggregate (unchanged calls)
    let aggregate_stats = compute_aggregate_stats(&scenario_results)
    let regressions = find_regressions(&scenario_results, &query_map)
    let latency_buckets = compute_latency_buckets(&scenario_results)
    let entry_rank_changes = compute_entry_rank_changes(&scenario_results)
    let cc_at_k_rows = compute_cc_at_k_scenario_rows(&scenario_results)
    let phase_stats = compute_phase_stats(&scenario_results)

    // Step 5: Render (ONE new parameter — profile_meta)
    // check_distribution_targets is called inline inside render_report for each
    // distribution-change profile. No pre-computation step here. (section5-dispatch.md)
    let md = render_report(
        &aggregate_stats,
        &phase_stats,
        &scenario_results,
        &regressions,
        &latency_buckets,
        &entry_rank_changes,
        &query_map,
        &cc_at_k_rows,
        &profile_meta,           // NEW (nan-010)
    )

    // Step 6: Write output (unchanged)
    // Step 7: Confirm written (unchanged)
    // Step 8: Return Ok(()) (unchanged — C-07, FR-29)
```

---

## Corrupt Sidecar Behavior

When `load_profile_meta` returns `Err`:
- `run_report` propagates the error via `?`
- `run_report` returns `Err(...)` to the CLI
- The CLI exits non-zero
- Error message is surfaced to stderr

This satisfies: "present but malformed → abort with non-zero exit" (ARCHITECTURE.md
Component 7, R-07 resolution, ADR-002 updated behavior).

Note: ADR-002 as written in the file contains older language about WARN+fallback. The
architecture document (Component 7) and the RISK-TEST-STRATEGY (R-07 resolution) supersede
this. The pseudocode follows the architecture. **See Open Questions.**

---

## Data Flow

Inputs to `load_profile_meta`:
- `dir: &Path` — the `--results` directory path passed to `run_report`

Outputs from `load_profile_meta`:
- `Ok(HashMap::new())` when `profile-meta.json` absent
- `Ok(HashMap<String, ProfileMetaEntry>)` when file present and valid
- `Err(...)` when file present but invalid JSON

The `HashMap<String, ProfileMetaEntry>` is passed to `render_report` as its single new
parameter. `render_report` calls `check_distribution_targets` inline for each
distribution-change profile (see section5-dispatch.md).

---

## Error Handling Summary

| Condition | Behavior |
|-----------|----------|
| `profile-meta.json` absent | `Ok(HashMap::new())` — backward-compat, no error |
| `profile-meta.json` present, unreadable | `Err(io_error)` → run_report aborts |
| `profile-meta.json` present, malformed JSON | `Err("profile-meta.json is malformed — re-run eval to regenerate...")` → run_report aborts |
| `profile-meta.json` present, valid | `Ok(HashMap<...>)` → processing continues |

`run_report` always returns `Ok(())` on success (C-07, FR-29). It only returns `Err` on
the corrupt-sidecar path, which causes a non-zero exit.

---

## Key Test Scenarios

Tests in `eval/report/tests_distribution_gate.rs`:

```
test_report_without_profile_meta_json:
    Setup: results dir with valid ScenarioResult JSON files, no profile-meta.json
    Call: run_report(results, None, out)
    Assert: returns Ok(())
    Assert: output report contains "## 5. Zero-Regression Check"
    Assert: no "Distribution Gate" text in output
    Assert: process conceptually exits 0 (function returns Ok)

test_distribution_gate_corrupt_sidecar_aborts:
    Setup: results dir with valid ScenarioResult JSON files,
           profile-meta.json containing truncated/invalid JSON: "{\"version\": 1, \"pro"
    Call: run_report(results, None, out)
    Assert: returns Err(...)
    Assert: error message contains "profile-meta.json is malformed"

test_distribution_gate_exit_code_zero (R-12):
    Setup: results dir with distribution-change profile where CC@k fails target
    Call: run_report(results, None, out)
    Assert: returns Ok(()) — not Err
    Assert: output report contains "FAILED"

Round-trip (R-10 sidecar schema):
    Setup: call write_profile_meta to produce profile-meta.json in a temp dir
           with distribution_change=true profile
    Call: run_report against that dir
    Assert: output report contains "Distribution Gate" for that profile
    Assert: output report contains the target values from the sidecar
```

---

## Open Questions

### OQ-1: Corrupt sidecar behavior discrepancy in ADR-002

ADR-002 (as written in the ADR file) states:
> "When profile-meta.json is present but malformed, run_report logs a WARN and falls
> back to empty map"

ARCHITECTURE.md Component 7 states:
> "If present but malformed JSON → return EvalError with message ... abort the report,
> and exit non-zero"

IMPLEMENTATION-BRIEF.md §load_profile_meta and RISK-TEST-STRATEGY R-07 both specify abort.

**The architecture document, implementation brief, and risk strategy all agree on abort.**
The ADR file contains stale language. This pseudocode follows the architecture (abort).

The implementation agent should note this discrepancy and implement abort behavior.
The ADR file does not need updating (the implementation brief and architecture are the
authoritative source; the ADR's Consequences section refers to an earlier iteration).

### OQ-2: `render.rs` line budget after Section 5 replacement

Current `render.rs`: 499 lines. Section 5 replacement adds a loop with
heading logic, replacing ~30 lines of existing Section 5 code with ~50+ lines. This will
likely push `render.rs` past 500 lines. The implementation agent must measure and extract
the zero-regression block into a private helper or a `render_zero_regression.rs` sibling
if needed to stay within the limit.

---

## Notes

- `serde_json::from_str::<ProfileMetaFile>` is the deserializer — using the same
  `ProfileMetaFile` type defined in `runner/profile_meta.rs`. This is the round-trip
  contract (R-10).
- The `.tmp` file check: `load_profile_meta` reads only `profile-meta.json`. A leftover
  `profile-meta.json.tmp` in the directory is not read.
- `std::fs::read_to_string` is used (synchronous) — `run_report` is already synchronous.
- `profile_meta` is passed as `&HashMap<String, ProfileMetaEntry>` — borrowed, not moved.
  Passed directly to `render_report`; no pre-computation step in `run_report`.
