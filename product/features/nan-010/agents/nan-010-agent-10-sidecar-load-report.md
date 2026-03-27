# Agent Report: nan-010-agent-10-sidecar-load

**Component**: Component 7 — Report Sidecar Load
**File modified**: `crates/unimatrix-server/src/eval/report/mod.rs`
**Test file extended**: `crates/unimatrix-server/src/eval/report/tests_distribution_gate.rs`

---

## Summary

Implemented Component 7 per pseudocode/report-sidecar-load.md and test-plan/report-sidecar-load.md.

### Changes Made

**`eval/report/mod.rs`**:
1. Added imports: `crate::eval::profile::EvalError`, `crate::eval::runner::profile_meta::{ProfileMetaEntry, ProfileMetaFile}`.
2. Added `pub(super) fn load_profile_meta(dir: &Path) -> Result<HashMap<String, ProfileMetaEntry>, EvalError>`:
   - Uses `match std::fs::read_to_string(&path)` with `io::ErrorKind::NotFound` guard.
   - NotFound arm: `Ok(HashMap::new())` — backward-compat (AC-11, ADR-002, knowledge #3585).
   - Other IO error arm: `Err(EvalError::Io(e))` — present-but-unreadable propagates.
   - Ok(contents) arm: `serde_json::from_str::<ProfileMetaFile>(&contents).map(...).map_err(...)` — corrupt sidecar aborts with `EvalError::ConfigInvariant("profile-meta.json is malformed — re-run eval to regenerate ...")`.
3. Added Step 3.5 in `run_report`: `let profile_meta = load_profile_meta(results)?;`
4. Passed `&profile_meta` as new final argument to `render_report` call.
5. Updated `run_report` doc comment to note `Err` path for corrupt sidecar.

**`eval/report/tests_distribution_gate.rs`** — added three Component 7 tests:
- `test_report_without_profile_meta_json` (AC-11, AC-14, R-15): absent file → `Ok(empty)`, pre-nan-010 JSON deserializes cleanly, `run_report` succeeds and renders "Zero-Regression Check".
- `test_distribution_gate_corrupt_sidecar_aborts` (R-07): truncated JSON → `Err` with "profile-meta.json is malformed" and "re-run eval to regenerate" substrings.
- `test_distribution_gate_exit_code_zero` (R-12): run_report returns `Ok(())` even with regressions present (gate outcome never affects exit code).

---

## Build

`cargo build -p unimatrix-server` — pass, 13 pre-existing warnings (unchanged).

## Tests

```
test eval::report::tests_distribution_gate::test_distribution_gate_corrupt_sidecar_aborts ... ok
test eval::report::tests_distribution_gate::test_distribution_gate_exit_code_zero ... ok
test eval::report::tests_distribution_gate::test_report_without_profile_meta_json ... ok
test eval::report::tests_distribution_gate::test_write_profile_meta_nonexistent_dir_returns_err ... ok
test eval::report::tests_distribution_gate::test_write_profile_meta_schema ... ok
test eval::report::tests_distribution_gate::test_write_profile_meta_schema_all_false ... ok
test eval::report::tests_distribution_gate::test_write_profile_meta_schema_empty_profiles ... ok
test eval::report::tests_distribution_gate::test_write_profile_meta_schema_tmp_not_read_as_sidecar ... ok

test result: ok. 8 passed; 0 failed
```

---

## Notes

### render_report signature mismatch

When this agent ran, `render_report` in `render.rs` did NOT yet have the new `profile_meta` parameter (C6 was running in parallel). However, the build passed — confirming that C6 had already landed its `render_report` signature change before this component's build ran. The `&profile_meta` argument wired here compiled correctly against C6's updated signature.

### OQ-1 (corrupt sidecar ADR-002 discrepancy)

The ADR-002 file contains stale language about WARN+fallback. The implementation follows ARCHITECTURE.md Component 7 (abort on corrupt). No ADR file update was made — per pseudocode OQ-1 note, the implementation brief and architecture are authoritative.

### `load_profile_meta` visibility

Set to `pub(super)` (not private `fn`) so the sibling test module `tests_distribution_gate` can call it directly for the `test_distribution_gate_corrupt_sidecar_aborts` test, which tests `load_profile_meta` in isolation.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "eval harness sidecar backward compat corrupt abort" — returned entries #3585 and #3582, both directly applicable. Applied knowledge package constraints as specified.
- Stored: attempted via `/uni-store-pattern` — blocked (agent lacks Write capability). Pattern discovered: "Use `match std::fs::read_to_string` with `io::ErrorKind::NotFound` guard rather than `path.exists()` pre-check for optional sidecar files — avoids TOCTOU and cleanly distinguishes absent vs. unreadable vs. corrupt."
