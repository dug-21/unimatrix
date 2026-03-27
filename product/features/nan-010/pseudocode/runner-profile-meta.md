# Component 3: Runner Profile Meta Sidecar

**Files**:
- `eval/runner/profile_meta.rs` — New module
- `eval/runner/mod.rs` — Modify (one call site added)

---

## Purpose

Produce and atomically write `profile-meta.json` to the eval output directory after all
scenario replay completes. The sidecar is the artifact boundary between `eval run` and
`eval report` for profile metadata. It enables `eval report` to know which profiles declared
`distribution_change = true` without re-reading TOML files or touching `ScenarioResult`.

---

## New File: `eval/runner/profile_meta.rs`

### Module-Level Comment

```
//! Profile metadata sidecar writer (nan-010).
//!
//! Produces `profile-meta.json` in the eval output directory after all scenario
//! replay completes. The sidecar carries distribution_change flag and
//! DistributionTargets per profile so that `eval report` can select the correct
//! Section 5 gate without re-reading TOMLs.
//!
//! Atomic write: write to `.tmp` then `fs::rename` (ADR-004).
//! Separate serde types from EvalProfile types (ADR-002).
```

### Serde Types (JSON representation)

These are distinct from `EvalProfile` / `DistributionTargets`. They exist solely for
serialization to and deserialization from `profile-meta.json`.

```
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// JSON representation of distribution targets in the sidecar.
// Separate from in-memory DistributionTargets (no serde on profile types per ADR-002).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DistributionTargetsJson {
    pub cc_at_k_min: f64,
    pub icd_min: f64,
    pub mrr_floor: f64,
}

// Per-profile entry in profile-meta.json.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProfileMetaEntry {
    pub distribution_change: bool,
    pub distribution_targets: Option<DistributionTargetsJson>,
}

// Top-level profile-meta.json structure.
// version is a top-level field, not per-entry (ADR-002, design decision #2).
#[derive(Serialize, Deserialize, Debug)]
pub struct ProfileMetaFile {
    pub version: u32,   // always 1
    pub profiles: HashMap<String, ProfileMetaEntry>,
}
```

### Function: `write_profile_meta`

```
pub fn write_profile_meta(
    profiles: &[EvalProfile],
    out: &Path,
) -> Result<(), EvalError>

    // Step 1: Build the ProfileMetaFile from EvalProfile slice
    profiles_map: HashMap<String, ProfileMetaEntry> = HashMap::new()

    for profile in profiles:
        entry = ProfileMetaEntry {
            distribution_change: profile.distribution_change,
            distribution_targets: match &profile.distribution_targets {
                None => None,
                Some(dt) => Some(DistributionTargetsJson {
                    cc_at_k_min: dt.cc_at_k_min,
                    icd_min: dt.icd_min,
                    mrr_floor: dt.mrr_floor,
                }),
            },
        }
        profiles_map.insert(profile.name.clone(), entry)

    meta_file = ProfileMetaFile {
        version: 1,
        profiles: profiles_map,
    }

    // Step 2: Serialize to JSON
    json_str = serde_json::to_string_pretty(&meta_file)
                   .map_err(|e| EvalError::ConfigInvariant(
                       format!("failed to serialize profile-meta.json: {e}")
                   ))?

    // Step 3: Atomic write — write to .tmp, then rename
    tmp_path  = out.join("profile-meta.json.tmp")
    final_path = out.join("profile-meta.json")

    fs::write(&tmp_path, &json_str)
        .map_err(EvalError::Io)?

    // Step 4: Rename (atomic on POSIX same-filesystem)
    match fs::rename(&tmp_path, &final_path):
        Ok(()) => {}
        Err(rename_err) =>
            // Cross-device fallback: fs::copy then fs::remove_file
            // (edge case: should not occur for local eval output dirs)
            fs::copy(&tmp_path, &final_path)
                .map_err(EvalError::Io)?
            fs::remove_file(&tmp_path)
                .map_err(EvalError::Io)?
            // If copy succeeded but remove failed, a .tmp artifact is left.
            // This is cosmetically untidy but functionally harmless (report reads
            // only profile-meta.json, not .tmp). Do not surface remove failure.

    Ok(())
```

---

## Modified File: `eval/runner/mod.rs`

### Module Declaration

Add to the `mod` block at the top of `runner/mod.rs`:
```
mod profile_meta;
pub use profile_meta::{ProfileMetaEntry, DistributionTargetsJson};
```

`ProfileMetaEntry` and `DistributionTargetsJson` must be re-exported so `report/mod.rs`
can import them without a cross-crate dependency on runner internals.

### Modified Function: `run_eval_async`

Add a single call site after `run_replay_loop` returns `Ok(())` and before the final
`eprintln!("eval run: complete")` line.

```
async fn run_eval_async(db, scenarios, profiles, k, out):

    // ... (existing: layer construction, NLI wait, skipped profiles, scenario load) ...

    // 4. Replay each scenario through each profile (unchanged)
    replay::run_replay_loop(&profiles, &layers, &scenario_records, k, out).await?

    // 5. NEW: Write profile metadata sidecar after replay completes (nan-010)
    // profiles is the full slice (including any NLI-skipped profiles — they still
    // have valid metadata that should be recorded).
    profile_meta::write_profile_meta(&profiles, out)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?

    eprintln!("eval run: complete. results in {}", out.display())
    Ok(())
```

Note on the profiles slice: `profiles` is the original parsed slice before NLI skip
filtering. Passing the full slice ensures all TOML-declared metadata is captured in the
sidecar, even for profiles that were skipped during replay. This matches the intent: the
sidecar records what was declared in the TOMLs.

However, if the run fails early (before `run_replay_loop` returns), `write_profile_meta`
is never called, leaving `profile-meta.json` absent. This is correct: absent = incomplete
run, `eval report` falls back to backward-compat mode.

---

## Data Flow

Inputs:
- `profiles: &[EvalProfile]` — the parsed profiles from `run_eval_async`
- `out: &Path` — output directory path

Outputs:
- `{out}/profile-meta.json` written on success
- `{out}/profile-meta.json.tmp` may exist on crash between write and rename (ignored by report)

---

## Error Handling

| Condition | Error |
|-----------|-------|
| JSON serialization failure | `EvalError::ConfigInvariant("failed to serialize profile-meta.json: {e}")` |
| `fs::write(.tmp)` failure | `EvalError::Io(e)` (disk full, permission denied) |
| `fs::rename` AND `fs::copy` failure | `EvalError::Io(e)` from `fs::copy` |

When `write_profile_meta` returns `Err`, `run_eval_async` propagates it as a `Box<dyn Error>`.
`run_eval` surfaces it to the CLI. The run exits non-zero, which is correct: the run completed
but the sidecar was not written. The operator must re-run to get a valid sidecar.

When the operator re-runs without cleaning the output directory, prior result files remain.
This is pre-existing behavior for partial runs and is out of scope.

---

## Key Test Scenarios

Tests in `eval/report/tests_distribution_gate.rs` (round-trip tests):

```
test_write_profile_meta_schema:
    Given: profiles = [
        EvalProfile { name="baseline", distribution_change=false, ... },
        EvalProfile { name="candidate", distribution_change=true,
                      distribution_targets=Some(DistributionTargets{0.60,1.20,0.35}) },
    ]
    Call: write_profile_meta(&profiles, &tmp_dir)
    Assert:
        profile-meta.json exists
        profile-meta.json.tmp does NOT exist
        parsed JSON has version=1
        parsed JSON profiles["baseline"].distribution_change == false
        parsed JSON profiles["baseline"].distribution_targets == null
        parsed JSON profiles["candidate"].distribution_change == true
        parsed JSON profiles["candidate"].distribution_targets.cc_at_k_min == 0.60
        parsed JSON profiles["candidate"].distribution_targets.icd_min == 1.20
        parsed JSON profiles["candidate"].distribution_targets.mrr_floor == 0.35

Atomic path test (inline):
    Create a valid profile-meta.json.tmp in the output dir with invalid content
    Run eval report against that dir
    Assert: report reads profile-meta.json (absent → backward-compat), NOT the .tmp

Round-trip (R-10):
    Write via write_profile_meta → read via load_profile_meta
    Assert: distribution_change=true profile renders Distribution Gate in Section 5
```

---

## Notes

- `serde_json::to_string_pretty` is used (not `to_string`) for human-readability of the
  sidecar file, consistent with the existing use in `skipped.json` write in `runner/mod.rs`.
- Import: `use crate::eval::profile::{EvalProfile, DistributionTargets}` in `profile_meta.rs`.
- The `EvalError` import: `use crate::eval::profile::EvalError` (same path used in runner/mod.rs).
- `fs` import: `use std::fs` (not `tokio::fs` — write is synchronous, called from async context
  via blocking I/O which is acceptable for a single file flush at run completion).
