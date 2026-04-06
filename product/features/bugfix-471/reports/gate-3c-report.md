# Gate 3c Report: bugfix-471

> Gate: 3c (Bug Fix Validation)
> Date: 2026-04-06
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Fix addresses root cause | PASS | Allowlist SQL change directly eliminates the denylist gap |
| No todo!/unimplemented!/TODO/FIXME introduced | PASS | Existing TODO(#409) is pre-existing, not introduced by this fix |
| All tests pass | PASS | 4539 unit + 4 new bug-specific + 22 smoke + 44 lifecycle + 23 edge_cases |
| No new clippy warnings in changed crates | PASS | 0 errors in unimatrix-server and unimatrix-store |
| No unsafe code introduced | PASS | No new unsafe blocks in diff |
| Fix is minimal | PASS | Only the two SQL sites + bind values changed; analytics.rs comment only |
| New tests would have caught original bug | PASS | Tests verify deprecated-endpoint edges are deleted, which would have failed against the old denylist SQL |
| Integration smoke tests passed | PASS | 22/22 |
| xfail markers have corresponding GH Issues | PASS | No new xfail markers introduced; all pre-existing markers have documented issues |
| Knowledge stewardship — investigator | WARN | Report exists (GH issue comment) but lacks a ## Knowledge Stewardship block |
| Knowledge stewardship — rust-dev | WARN | No rust-dev agent report found (no filesystem file, no GH comment) |
| Knowledge stewardship — verifier | PASS | ## Knowledge Stewardship block present with Queried and Stored entries |
| File size (500-line limit) | WARN | background.rs is 4100 lines — pre-existing condition, this fix added 192 lines of tests |

## Detailed Findings

### Fix Addresses Root Cause

**Status**: PASS

**Evidence**: The diff shows two symmetric changes:

Production SQL (background.rs lines 513–519):
```
- WHERE source_id NOT IN (SELECT id FROM entries WHERE status != ?1)
+ WHERE source_id NOT IN (SELECT id FROM entries WHERE status = ?1)
- .bind(Status::Quarantined as u8 as i64)
+ .bind(Status::Active as u8 as i64)
```

Test helper SQL (background.rs lines 2829–2835): identical change pattern.

The old denylist `status != Quarantined` returned Active + Deprecated entries as the "protected" set, meaning deprecated-endpoint edges were never removed. The new allowlist `status = Active` protects only Active entries, so Deprecated, Quarantined, and non-existent endpoint edges are all deleted. This is exactly the root cause identified in the bug report and investigator analysis.

### No Placeholder Code Introduced

**Status**: PASS

**Evidence**: `grep "todo!|unimplemented!|TODO|FIXME"` in the diff shows zero new introductions. The existing `// TODO(#409)` comment at line 980 is confirmed pre-existing (present in HEAD~1 at the same line number).

### All Tests Pass

**Status**: PASS

**Evidence** (from tester report `product/features/bugfix-471/agents/471-agent-2-verify-report.md`):
- 4 new bug-specific tests: all PASS
- 4539/4539 unit tests: PASS
- 22/22 smoke integration: PASS
- lifecycle suite: 44 passed, 5 xfailed (pre-existing), 2 xpassed (pre-existing)
- edge_cases suite: 23 passed, 1 xfailed (pre-existing GH#111)
- Build: `Finished dev profile` with no errors

### No New Clippy Warnings in Changed Crates

**Status**: PASS

**Evidence**: Running `cargo clippy -p unimatrix-server -- -D warnings` and `cargo clippy -p unimatrix-store -- -D warnings` individually produces zero errors. The 58 workspace-level errors are exclusively in `unimatrix-observe` and `unimatrix-engine`, confirmed pre-existing. The tester confirmed this via Unimatrix entry #3257 (clippy triage procedure).

### No Unsafe Code Introduced

**Status**: PASS

**Evidence**: The diff contains no `unsafe` blocks. The `unsafe` references in background.rs are in comments explaining why unsafe env-var manipulation was avoided — all pre-existing.

### Fix Is Minimal

**Status**: PASS

**Evidence**: The diff is tightly scoped:
- background.rs: Two SQL string changes (denylist → allowlist), two bind value changes (Quarantined → Active), four new test functions (192 lines of tests)
- analytics.rs: Comment update only — removed forward-reference to GH #477 (resolved/no-longer-applicable), cited Unimatrix entry #3979

No unrelated refactoring, no scope additions.

### New Tests Would Have Caught Original Bug

**Status**: PASS

**Evidence**: Each of the 4 new tests inserts a deprecated entry, creates an edge to/from it, runs `run_graph_edges_compaction`, and asserts the edge is deleted. Against the old SQL (denylist `status != Quarantined`), the deprecated entry's ID would appear in the subquery result (since status=1 != 3=Quarantined), so `NOT IN` would evaluate false and the DELETE would skip that edge — the assertion `count_graph_edges == 0` would fail. The tests are causally tied to the exact bug.

### Integration Smoke Tests

**Status**: PASS

**Evidence**: 22/22 passing per tester report. All test names listed in report match expected suite.

### xfail Markers

**Status**: PASS

**Evidence**: No new xfail markers introduced by this fix. All observed xfails are pre-existing: GH#111 (edge_cases), GH#406 (lifecycle), and several tick-timing-dependent tests.

### Knowledge Stewardship — Investigator

**Status**: WARN

**Evidence**: The investigator report was posted as a GitHub issue comment (GH #471, comment IC_kwDORTRSjM75uvJ3). The comment contains detailed root cause analysis, code path trace, proposed fix, and risk assessment. However, it does not contain a `## Knowledge Stewardship` block with `Queried:` or `Stored:` entries as required. The omission means there is no evidence the investigator queried Unimatrix before diagnosing (though the analysis quality is high and the fix references entry #3908 — suggesting pattern awareness).

No agent report file exists at `product/features/bugfix-471/agents/471-agent-1-investigate-report.md`.

**Issue**: Missing `## Knowledge Stewardship` block in investigator report. Not a blocking failure (the substantive work is correct), but stewardship compliance is incomplete.

### Knowledge Stewardship — Rust-Dev

**Status**: WARN

**Evidence**: No rust-dev agent report found anywhere on the filesystem or as a GH issue/PR comment. The implementation appears to have been performed without a separate rust-dev agent report being filed. The fix is correct and clean, but there is no documented evidence that the implementer queried Unimatrix patterns before implementation or recorded what was stored.

**Issue**: Missing rust-dev agent report entirely. Cannot verify Queried/Stored entries.

### Knowledge Stewardship — Verifier

**Status**: PASS

**Evidence** (from `product/features/bugfix-471/agents/471-agent-2-verify-report.md` lines 123–126):
```
## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned entry #4156 (allowlist lesson), #3910, #3257
- Stored: nothing novel to store — entry #4156 already captures the allowlist lesson
```

Both `Queried:` and `Stored:` (with reason) entries are present.

### File Size Limit

**Status**: WARN

**Evidence**: `background.rs` is 4100 lines. The 500-line limit per the Rust workspace rules is far exceeded. This is a pre-existing condition — the file was 3908 lines before this fix. The fix added 192 lines (4 test functions). The oversize condition is not caused by this fix and has a separate tracking obligation, but is noted for completeness.

---

## Rework Required

None. All FAILs are WARNs (non-blocking). The fix is technically correct, minimal, well-tested, and all required test suites pass.

---

## Warnings Summary

| Warning | Agent | Severity |
|---------|-------|----------|
| Investigator report missing `## Knowledge Stewardship` block | investigator | Low — substantive work correct, stewardship record incomplete |
| Rust-dev agent report absent (no file, no GH comment) | rust-dev | Low — fix is clean, but no documented Queried/Stored evidence |
| `background.rs` exceeds 500-line limit (4100 lines, pre-existing) | N/A | Pre-existing, not caused by this fix |

---

## Knowledge Stewardship

- Stored: nothing novel to store -- the allowlist-vs-denylist SQL lesson is already captured in entry #4156 (which the verifier confirmed). The missing-report stewardship pattern is covered by existing validation conventions.
