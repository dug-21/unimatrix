# Gate Bugfix Report: bugfix-279

> Gate: Bugfix Validation
> Date: 2026-03-16
> Result: PASS (1 warning)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Fix addresses root cause | PASS | Constant + LIMIT ?2 bind parameter eliminates the 10,000-row hold |
| No todo!/unimplemented!/TODO/FIXME | PASS | None found in changed code |
| All tests pass | PASS | 6 new tests pass; 2533 workspace total, 0 failures |
| No new clippy warnings | PASS | unimatrix-server clean; pre-existing errors in other crates are unrelated |
| No unsafe code introduced | PASS | No unsafe blocks added |
| Fix is minimal | PASS | Only `background.rs` changed in commit; xfail marker in test_adaptation.py is uncommitted (see WARN) |
| New tests would have caught original bug | PASS | test_extraction_batch_size_constant_value asserts 1000 != 10000; batch-cap tests enforce the bound |
| Integration smoke tests passed | PASS | 19/20 passed; 1 XFailed (pre-existing GH#111) |
| XFail markers have corresponding GH Issues | PASS | @pytest.mark.xfail references GH#111 which is open and correctly titled |
| Knowledge stewardship: investigator | PASS | ## Knowledge Stewardship block present; Queried + Stored entries present |
| Knowledge stewardship: rust-dev | PASS | ## Knowledge Stewardship block present; Queried + "nothing novel" with reason |
| xfail marker committed | WARN | test_adaptation.py xfail marker is present in working tree but not committed |
| Batch size matches investigator recommendation | WARN | Investigator recommended 500; implementation uses 1000 (no documented rationale for deviation) |
| File size (pre-existing) | WARN | background.rs is 2283 lines (was 2088 before fix); 500-line limit is pre-existing and not introduced by this fix |

## Detailed Findings

### Fix Addresses Root Cause

**Status**: PASS

**Evidence**: The root cause was `LIMIT 10000` as a magic literal inside a single `spawn_blocking` closure that held `Mutex<Connection>` for the entire row iteration. The fix:
1. Adds `const EXTRACTION_BATCH_SIZE: i64 = 1000` at line 62, alongside the other module-level constants.
2. Extracts `fetch_observation_batch(store, watermark)` as a standalone synchronous helper (lines 874-943) using `LIMIT ?2` with `rusqlite::params![watermark as i64, EXTRACTION_BATCH_SIZE]`.
3. The first `spawn_blocking` in `extraction_tick()` (line 961) now delegates entirely to this helper.

The mutex hold is now bounded to 1000 rows — a 10x reduction from 10,000. The watermark mechanism is unchanged and correct; any backlog beyond 1000 rows is deferred to the next tick with no data loss.

### No Stubs or Placeholders

**Status**: PASS

**Evidence**: Searched `background.rs` for `todo!()`, `unimplemented!()`, `TODO`, `FIXME`. None found. Two `.unwrap()` calls exist but both are in test functions (`#[test]` annotated) where this is acceptable.

### All Tests Pass

**Status**: PASS

**Evidence**: Independently verified with `cargo test -p unimatrix-server`:
- `test_fetch_observation_batch_first_batch_capped_at_batch_size` — PASS
- `test_fetch_observation_batch_second_call_advances_watermark` — PASS
- `test_fetch_observation_batch_remainder_processed_on_third_tick` — PASS
- `test_fetch_observation_batch_empty_store_returns_empty` — PASS
- `test_fetch_observation_batch_no_reprocessing_past_watermark` — PASS
- `test_extraction_batch_size_constant_value` — PASS

Workspace total: 2533 passed, 0 failed (verify agent report). Build is clean: `cargo build -p unimatrix-server` compiles with no errors.

### No New Clippy Warnings

**Status**: PASS

**Evidence**: `cargo clippy -p unimatrix-server -- -D warnings` produces no `error[E...]` entries from unimatrix-server itself. Pre-existing clippy errors in `unimatrix-engine` and `unimatrix-observe` are unrelated to this change and were present before the fix.

### No Unsafe Code Introduced

**Status**: PASS

**Evidence**: The word "unsafe" appears only in comments (explaining why `std::env::set_var` is avoided in tests). No `unsafe { }` blocks were introduced.

### Fix is Minimal

**Status**: PASS

**Evidence**: The commit `163af7c` modifies exactly one file: `crates/unimatrix-server/src/background.rs`. The diff is 251 insertions and 56 deletions, with the additions being the new helper function (79 lines), the new tests (165 lines), and the constant declaration (7 lines with doc comment). No changes to store layer, extraction rules, or unrelated logic.

Note: `product/test/infra-001/suites/test_adaptation.py` has an uncommitted xfail marker added by the verify agent — see WARN below.

### New Tests Would Have Caught Original Bug

**Status**: PASS

**Evidence**: `test_extraction_batch_size_constant_value` asserts `EXTRACTION_BATCH_SIZE == 1000`, which would fail if someone removed the constant or reverted to the literal. The five batch/watermark tests verify the LIMIT is honored at exactly `EXTRACTION_BATCH_SIZE` rows — a test against `LIMIT 10000` would produce 1000 rows returned for a 1200-row store, not 10000, so the behavior was testable.

The investigator's recommended test scenario (1200-row backlog, verify first call returns exactly EXTRACTION_BATCH_SIZE, second call returns remainder) is fully implemented as AC-01 and AC-03.

### Integration Smoke Tests

**Status**: PASS

**Evidence** (from verify agent): 19/20 smoke tests passed. 1 XFailed (`test_volume_with_adaptation_active`) with `@pytest.mark.xfail(reason="Pre-existing: GH#111 — rate limit blocks volume test")`. GH#111 is confirmed open and titled "[infra-001] test_store_1000_entries: rate limit blocks volume test". The xfail is correctly attributed to a pre-existing issue.

Availability suite: 6/6 passed including `test_concurrent_ops_during_tick` and `test_read_ops_not_blocked_by_tick` — directly confirming the fix does not degrade availability.

### Knowledge Stewardship

**Status**: PASS

**Investigator** (279-investigator-report.md): `## Knowledge Stewardship` block present.
- Queried: entry references #735, #1367, #1688 via `context_search` and `uni-knowledge-lookup`.
- Stored: entry #1736 "Extraction tick batch size controls mutex hold duration: EXTRACTION_BATCH_SIZE constant pattern" via `/uni-store-lesson`.

**Fix agent** (279-agent-1-fix-report.md): `## Knowledge Stewardship` block present.
- Queried: attempted `/uni-query-patterns`; server unavailable, non-blocking.
- Stored: "nothing novel to store — pattern already captured in entry #1736 by investigator." Reason provided.

**Verify agent** (279-agent-2-verify-report.md): `## Knowledge Stewardship` block present.
- Queried: attempted `/uni-knowledge-search`; server unavailable, non-blocking.
- Stored: "nothing novel to store — xfail triage pattern is a known procedure." Reason provided.

All three blocks are complete and compliant.

### WARN: XFail Marker Uncommitted

**Status**: WARN

**Evidence**: `git status` shows `M product/test/infra-001/suites/test_adaptation.py` — the xfail marker added by the verify agent is present in the working tree but was not included in commit `163af7c`. The marker itself is correct (GH#111 reference, correct reason text), but it needs to be committed before the PR is merged to prevent the test from appearing as a failure on CI.

**Fix**: Stage and commit `product/test/infra-001/suites/test_adaptation.py` with a `chore(test):` commit message before merging.

### WARN: Batch Size Deviates from Investigator Recommendation Without Documentation

**Status**: WARN

**Evidence**: The investigator recommended `EXTRACTION_BATCH_SIZE = 500` with the rationale that "500 rows bounds hold time to ~50-200ms at worst." The implementation chose 1000 with no documented rationale in the fix report or code comment for the deviation. Both values are correct directionally (10x or 5x reduction from 10,000), and the test `test_extraction_batch_size_constant_value` asserts the chosen value is exactly 1000 to prevent silent regression.

**Fix**: No code change required. Consider adding a note to the PR description explaining why 1000 was chosen over 500 (e.g., "1000 provides sufficient improvement while reducing the number of ticks needed to clear a typical backlog").

### WARN: background.rs Line Count Exceeds 500-Line Gate Limit (Pre-existing)

**Status**: WARN

**Evidence**: `background.rs` is 2283 lines (was 2088 before this fix). The 500-line limit was already violated before bugfix-279. The fix added 195 lines, primarily tests. This is a pre-existing structural debt and not introduced by this PR. It is flagged for awareness only.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for extraction tick batch size and mutex contention validation — entry #1736 is the directly relevant lesson stored by the investigator.
- Stored: nothing novel to store — this gate validation confirms a clean fix with one pending commit (xfail marker). No new generalizable pattern beyond what #1736 already captures.
