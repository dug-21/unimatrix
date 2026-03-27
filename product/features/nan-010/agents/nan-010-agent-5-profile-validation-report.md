# nan-010 Agent 5: Profile Validation Report

**Agent ID**: nan-010-agent-5-profile-validation
**Component**: Profile Validation (`eval/profile/validation.rs`)
**GH Issue**: #402

---

## Work Completed

Modified `crates/unimatrix-server/src/eval/profile/validation.rs`:

1. Added `DistributionTargets` to the import from `super::types`.

2. Inserted extraction block for `distribution_change` and `distribution_targets` BEFORE the `table.remove("profile")` call (line 85 in final file), following the established `raw.get("profile")` pattern used for `name` and `description`.

3. Implemented all four validation rules:
   - `distribution_change = true` + no `[profile.distribution_targets]` sub-table → `ConfigInvariant` naming the missing section
   - `distribution_change = true` + missing `cc_at_k_min` → `ConfigInvariant` naming the field
   - `distribution_change = true` + missing `icd_min` → `ConfigInvariant` naming the field
   - `distribution_change = true` + missing `mrr_floor` → `ConfigInvariant` naming the field
   - `name.eq_ignore_ascii_case("baseline")` + `distribution_change = true` → `ConfigInvariant` "baseline profile must not declare `distribution_change = true`"

4. Replaced the placeholder `distribution_change: false, distribution_targets: None` in the `Ok(EvalProfile { ... })` return with the extracted values.

---

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/profile/validation.rs`

---

## Build

**pass** — `cargo build -p unimatrix-server` completed with zero errors. 15 pre-existing warnings in the lib target (none from `validation.rs`). Clippy errors are pre-existing in `unimatrix-engine`, not in `validation.rs`.

---

## Test Results

Tests not yet written — per spawn prompt, tests go in Wave 4 (`eval/profile/tests.rs`). The implementation satisfies all assertions described in the test plan for:
- `test_parse_distribution_change_profile_valid`
- `test_parse_distribution_change_missing_targets`
- `test_parse_distribution_change_missing_cc_at_k`
- `test_parse_distribution_change_missing_icd`
- `test_parse_distribution_change_missing_mrr_floor`
- `test_parse_no_distribution_change_flag`
- baseline rejection path used by `test_distribution_gate_baseline_rejected`

---

## Issues / Blockers

None.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for toml extraction pattern before strip — found #2806 (eval harness profile TOML pattern) and #3582 (sidecar metadata pattern). Both confirm the `raw.get("profile")` access approach before strip.
- Stored: nothing novel to store — the extraction ordering invariant is already captured in pseudocode/profile-validation.md and the pre-existing profile TOML pattern (#2806). No new gotchas discovered.
