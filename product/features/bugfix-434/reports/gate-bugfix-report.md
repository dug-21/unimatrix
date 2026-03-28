# Gate Bugfix Report: bugfix-434

> Gate: Bug Fix Validation
> Date: 2026-03-28
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed (not just symptoms) | PASS | Default constant and serde default fn both lowered to 0.6 |
| No todo!/unimplemented!/TODO/FIXME/placeholders | PASS | Zero matches in changed file |
| All tests pass | PASS | 3834 unit passed, 0 failed |
| No new clippy warnings | PASS | Zero hits in changed file; pre-existing warnings in auth.rs unrelated |
| No unsafe code introduced | PASS | Zero unsafe blocks in config.rs |
| Fix is minimal (no unrelated changes) | PASS | Single file, 21 insertions / 7 deletions — all directly on the threshold |
| New test would have caught the original bug | PASS | Regression guard fails if default is >= 0.7 |
| Integration smoke tests passed | PASS | 20/20 |
| Lifecycle + adaptation integration suites | PASS | 38+9 passed, 3 xfailed/xpassed all pre-existing |
| xfail markers have corresponding GH Issues | PASS | All xfails reference GH#405 or GH#406 (pre-existing) |
| Knowledge stewardship — investigator report | PASS | Queried + Stored entries present |
| Knowledge stewardship — rust-dev report | WARN | Queried entry present; Stored declined with reason — acceptable |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: The diagnosis named two places that needed changing: the `Default` impl value and the `default_supports_edge_threshold()` serde function. The diff confirms both were lowered from 0.7 to 0.6:

- `Default` impl line 465: `supports_edge_threshold: 0.6`
- `default_supports_edge_threshold()` line 563-565: returns `0.6`

The TOML serde path and the code path now agree. The cross-field invariant (`supports_candidate_threshold < supports_edge_threshold`) is preserved: 0.5 < 0.6.

### No Placeholder Code

**Status**: PASS

**Evidence**: Grep for `TODO`, `FIXME`, `todo!`, `unimplemented!` in config.rs returns zero matches.

### All Tests Pass

**Status**: PASS

**Evidence** (from 434-agent-2-verify-report.md):
- unimatrix-server lib: 2269 passed, 0 failed
- Full workspace: ~3834 passed, 0 failed
- One transient failure (`col018_topic_signal_null_for_generic_prompt`) did not reproduce on re-run; documented as pre-existing flaky test, lesson stored as entry #3714.

### No New Clippy Warnings

**Status**: PASS

**Evidence**: Verifier ran `cargo clippy --workspace -- -D warnings`; zero hits in config.rs. Pre-existing warnings in `crates/unimatrix-engine/src/auth.rs` (collapsible_if, manual char comparison) were last modified in col-006/crt-014 and are unrelated to this fix.

### No Unsafe Code

**Status**: PASS

**Evidence**: Grep for `unsafe` in config.rs returns zero matches. The fix touches only constant literals and doc comments.

### Fix Is Minimal

**Status**: PASS

**Evidence**: Commit d580235 touches exactly one file (`crates/unimatrix-server/src/infra/config.rs`, 21 insertions / 7 deletions). Changes are:
1. Doc comment update on `supports_edge_threshold` field (updated rationale, references #434)
2. Default impl value: 0.7 → 0.6
3. `default_supports_edge_threshold()` return value: 0.7 → 0.6
4. Two existing test assertions updated to match new default (0.7 → 0.6)
5. New regression guard test added

Validation boundary tests that use explicit `supports_edge_threshold: 0.7` struct spreads were correctly left untouched — those test the validation logic, not the default.

### New Test Would Have Caught the Original Bug

**Status**: PASS

**Evidence**: `test_write_inferred_edges_default_threshold_yields_edges_at_0_6` asserts `InferenceConfig::default().supports_edge_threshold < 0.7_f32`. Had this test existed before the fix, it would have failed with the old default of 0.7 (7 is not less than 7). The test is a direct regression guard for the exact value change.

### Integration Tests

**Status**: PASS

**Evidence**:
- Smoke: 20/20 passed
- Lifecycle suite: 38 passed, 2 xfailed, 1 xpassed, 0 failed
- Adaptation suite: 9 passed, 1 xfailed, 0 failed

The xpassed result is `test_search_multihop_injects_terminal_active` (GH#406, pre-existing xfail). That test unexpectedly passing is a pre-existing intermittent behavior unrelated to this fix. The xfails all reference GH#405 or GH#406 which were filed before this fix.

### xfail Markers Have Corresponding GH Issues

**Status**: PASS

**Evidence**: All xfail markers in the suites reference pre-existing issues: GH#405 (deprecated confidence background scoring timing) and GH#406 (find_terminal_active multi-hop traversal not implemented). No new xfail markers were added by this fix.

### Knowledge Stewardship — rust-dev Report (434-agent-1-fix)

**Status**: WARN

**Evidence**: The `## Knowledge Stewardship` block is present. `Queried` entry explains why briefing was skipped (purely mechanical constant change, self-contained bug report). `Stored` entry declines with reason: "the fix is a single constant lowering; the underlying principle is already captured in the updated doc comment."

The decline reason is present but thin — "already in the doc comment" is marginal justification for skipping a lesson store about corpus-driven threshold calibration. However, the verifier (434-agent-2-verify) stored lesson entry #3714 covering the flaky test pattern, and the principle is explicitly documented inline in config.rs. This is WARN, not FAIL.

### Knowledge Stewardship — verifier Report (434-agent-2-verify)

**Status**: PASS

**Evidence**: `## Knowledge Stewardship` block present. `Queried` entries: #3713 (supports_edge_threshold default lesson — directly relevant), #3657 (write_edges_with_cap decision), #2326 (bug fix verification patterns). `Stored`: entry #3714 via `/uni-store-lesson`.

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store — single-file constant fix with clean pass; no recurring gate failure pattern detected.
