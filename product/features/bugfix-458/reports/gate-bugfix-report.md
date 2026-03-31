# Gate Bugfix Report: bugfix-458

> Gate: Bug Fix Validation
> Date: 2026-03-31
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | SQL predicate extended to filter status=3, not just absent entries |
| No stubs / placeholders | PASS | No todo!/unimplemented!/TODO/FIXME added by this PR |
| All tests pass | PASS | 2605 passed, 0 failed; 2 new quarantine-specific tests pass |
| No new clippy warnings | PASS | Pre-existing errors in unrelated crates; zero new warnings from changed code |
| No unsafe code | PASS | No unsafe blocks introduced |
| Fix is minimal | PASS | Single file changed: background.rs (+122 lines, all test code + SQL fix) |
| New tests would catch original bug | PASS | Both new tests fail against the old SQL, pass against the new |
| Integration smoke tests | PASS | 22/22 smoke tests passed |
| xfail markers handled correctly | PASS | No xfail markers added; pre-existing xfails correctly attributed to existing issues |
| Investigator stewardship | WARN | Stewardship block missing from GH issue comment; entries #3906 and #3907 confirm knowledge was stored |
| Rust-dev (458-agent-1-fix) stewardship | PASS | Queried + Stored (#3908) documented in agent report |
| Tester (458-agent-2-verify) stewardship | PASS | Queried + "nothing novel" with reason documented in agent report |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: The diagnosed root cause was `NOT IN (SELECT id FROM entries)` — quarantined entries exist in `entries` (status=3) so the condition never matched them. The fix changes both the production SQL (background.rs:519–525) and the test helper (background.rs:2934–2946) to:

```sql
DELETE FROM graph_edges
WHERE source_id NOT IN (SELECT id FROM entries WHERE status != ?1)
   OR target_id NOT IN (SELECT id FROM entries WHERE status != ?1)
```

with `.bind(Status::Quarantined as u8 as i64)`. This aligns the compaction DELETE with the VECTOR_MAP prune pass in status.rs, which was the correct precedent.

### No Stubs / Placeholders

**Status**: PASS

**Evidence**: `git diff origin/main..HEAD` shows only lines added by the fix. None of the added lines contain `todo!`, `unimplemented!`, `TODO`, `FIXME`, or `unsafe`. One pre-existing `TODO(#409)` comment at line 977 (deferred work referencing GH #409) was already present on main — it is not in the diff.

### All Tests Pass

**Status**: PASS

**Evidence**:
- `cargo test -p unimatrix-server`: 2605 passed, 0 failed (including the 2 new tests)
- `test_background_tick_compaction_removes_quarantined_source_edges`: ok
- `test_background_tick_compaction_removes_quarantined_target_edges`: ok
- Full workspace: 0 failures (tester report)

### No New Clippy Warnings

**Status**: PASS

**Evidence**: Tester verified `cargo clippy --workspace -- -D warnings`. Pre-existing errors exist in `unimatrix-engine` and `unimatrix-observe`; confirmed pre-existing by `git diff origin/main..HEAD -- crates/unimatrix-engine/ crates/unimatrix-observe/` showing 0 diff. No new warnings in `unimatrix-server` from this change.

### No Unsafe Code

**Status**: PASS

**Evidence**: `git diff origin/main..HEAD` grep for `unsafe` returns no added lines.

### Fix Is Minimal

**Status**: PASS

**Evidence**: `git diff --name-only origin/main..HEAD` shows exactly one file: `crates/unimatrix-server/src/background.rs`. The +122 lines are: 2 lines of production SQL fix, 4 lines of test helper SQL fix plus sync comment, and 114 lines of two new tests. No unrelated changes.

### New Tests Would Catch Original Bug

**Status**: PASS

**Evidence**: Both new tests insert a quarantined entry (status=3) and assert its edges are deleted by `run_graph_edges_compaction`. Against the old SQL (`NOT IN (SELECT id FROM entries)` with no status filter), the quarantined entry exists in `entries` so the DELETE would leave the edges in place, causing `count_graph_edges` to return 1 instead of 0 — the assertions would fail. The tests are causally connected to the diagnosed root cause.

### Integration Smoke Tests

**Status**: PASS

**Evidence**: 22/22 smoke tests passed in 191s. Lifecycle suite: 41 passed, 2 xfailed (pre-existing), 1 xpassed (GH#406 pre-existing xfail that now passes — unrelated to this fix, no action needed here).

### xfail Markers Handled Correctly

**Status**: PASS

**Evidence**: No new xfail markers were added by this fix. The 2 existing xfailed tests (`test_auto_quarantine_after_consecutive_bad_ticks`, `test_dead_knowledge_entries_deprecated_by_tick`) remain correctly attributed to pre-existing issues. The 1 xpassed test (`test_search_multihop_injects_terminal_active`, GH#406) is an existing xfail that began passing; the fix for GH#406 should remove that marker — this is not within the scope of this bug fix.

### Investigator Knowledge Stewardship

**Status**: WARN

**Evidence**: The investigator report was posted as GH issue comment https://github.com/dug-21/unimatrix/issues/458#issuecomment-4159084160. That comment contains no `## Knowledge Stewardship` block. However, Unimatrix entries #3906 (lesson-learned, created 2026-03-31T00:52:09Z) and #3907 (decision/ADR, created 2026-03-31T00:55:07Z) confirm the investigator stored relevant knowledge before the fix agent ran. The stewardship was performed; the report block was omitted.

This is a WARN (not FAIL) because the actual stewardship actions (store calls) are evidenced by the Unimatrix entries themselves.

### Rust-Dev (458-agent-1-fix) Knowledge Stewardship

**Status**: PASS

**Evidence**: Agent report at `/product/features/bugfix-458/agents/458-agent-1-fix-report.md` contains:
```
## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned entries #3906, #3907, #3762, #3883
- Stored: entry #3908 "GRAPH_EDGES compaction DELETE must filter quarantined entries..." via /uni-store-pattern
```

### Tester (458-agent-2-verify) Knowledge Stewardship

**Status**: PASS

**Evidence**: Tester report contains:
```
## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — entries #3906, #3907, #3908
- Stored: nothing novel to store — fix agent already stored the relevant pattern and lesson
```
Reason provided for "nothing novel" is adequate.

## Rework Required

None. The WARN on investigator stewardship does not block merge — the knowledge was stored; the report format was incomplete.

## Knowledge Stewardship

- Stored: nothing novel to store — this is a straightforward single-file SQL fix; no recurring gate failure patterns to capture beyond what the fix agent already stored in #3906/#3907/#3908.
