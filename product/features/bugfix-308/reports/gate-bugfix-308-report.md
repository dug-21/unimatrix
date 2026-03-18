# Gate Bugfix Report: bugfix-308

> Gate: Bugfix Validation (v2 — stewardship rework)
> Date: 2026-03-18
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Fix addresses root cause — all 5 sites converted | PASS | All 5 `log_event()` → `log_event_async()` fire-and-forget conversions confirmed in diff |
| No todo!/unimplemented!/TODO/FIXME/placeholders | PASS | None found in either changed file |
| All tests pass (new + suite) | PASS | 2 new regression tests pass; 10 failures are pre-existing GH #303 (pool timeout in concurrent test runs) |
| No new clippy warnings in changed files | PASS | Pre-existing warnings in `unimatrix-store` (18) and `audit_fire_and_forget` (line 402) unchanged; 0 new warnings in the 5 fixed sites |
| No unsafe code introduced | PASS | No unsafe blocks added |
| Fix is minimal — no unrelated changes | PASS | Only 2 files changed, 154 lines added, 22 removed; all changes are the 5 audit site conversions + test updates |
| New tests would have caught the original bug | PASS | `test_insert_with_audit_does_not_block_under_concurrent_writes` fires 10 concurrent inserts with 10s timeout; would have failed under the blocking `log_event()` |
| Integration smoke tests pass | PASS | 53 integration tests pass across export, pipeline_e2e, migration, and infra suites; import_integration failures are pre-existing GH #303 |
| xfail markers have corresponding GH Issues | PASS | No new xfail markers added in this PR |
| Knowledge stewardship — Phase 1 investigator report | PASS | Addendum comment `IC_kwDORTRSjM7zfnx9` contains `## Knowledge Stewardship` with `Queried:` and `Stored:` entries |
| Knowledge stewardship — Phase 2 rust-dev report | PASS | Addendum comment `IC_kwDORTRSjM7zfn4T` contains `## Knowledge Stewardship` with `Queried:` and `Stored:`/`Declined:` entries |

## Detailed Findings

### Fix Addresses Root Cause — All 5 Sites Converted

**Status**: PASS

**Evidence**: `git show c7b83a2 -- crates/unimatrix-server/src/server.rs` and `background.rs` show all 5 sites converted:

- `server.rs:insert_with_audit()` — `self.audit.log_event(...)?` → `tokio::spawn(async move { let _ = audit.log_event_async(...).await; })`
- `server.rs:correct_with_audit()` — same conversion
- `server.rs:quarantine_or_restore_with_audit()` — same conversion
- `background.rs:emit_tick_skipped_audit()` — same conversion
- `background.rs:emit_auto_quarantine_audit()` — same conversion

Residual `log_event()` calls at server.rs lines 403 and 406 are inside the pre-existing `audit_fire_and_forget()` helper which uses `spawn_blocking` (not `block_in_place`). This helper was not changed by this PR and is not part of the reported 5 sites.

### No Placeholder Functions

**Status**: PASS

**Evidence**: `grep -n "todo!\|unimplemented!\|TODO\|FIXME"` on both changed files returns empty.

### All Tests Pass

**Status**: PASS

**Evidence**:
- `server::tests::test_insert_with_audit_does_not_block_under_concurrent_writes` — ok
- `server::tests::test_quarantine_restore_audit_does_not_block` — ok
- Suite total: 1359 passed, 10 failed (all `import::tests::*` and `mcp::identity::tests::*` — pool timeout on concurrent test runs, confirmed pre-existing GH #303)
- Integration suites: 7 pipeline_e2e + 16 export_integration + 14 migration + 6 infra + others = all pass; 12 import_integration failures confirmed pre-existing GH #303

### No New Clippy Warnings

**Status**: PASS

**Evidence**: `cargo clippy --package unimatrix-server` shows the warning at `server.rs:402` ("non-binding `let` on a future") is on the pre-existing `audit_fire_and_forget()` helper, unchanged by this PR. The 5 new `tokio::spawn(async move { let _ = ... })` sites at lines 457–465, 506–514, and 814–820 of server.rs generate no clippy warnings. The `unimatrix-store` 18 pre-existing warnings are in an unchanged crate.

### No Unsafe Code

**Status**: PASS

**Evidence**: No `unsafe` blocks in the diff. Background.rs comments referencing "unsafe" are comments about `std::env::set_var` restrictions, not unsafe blocks.

### Fix Is Minimal

**Status**: PASS

**Evidence**: The commit touches exactly 2 files (confirmed with `git show --stat`): 154 lines added, 22 removed. Additions are the 5 site conversions, GH reference comments, and test updates. No unrelated modifications to logic, imports, or structure.

### New Tests Would Have Caught the Bug

**Status**: PASS

**Evidence**: `test_insert_with_audit_does_not_block_under_concurrent_writes` spawns 10 concurrent `insert_with_audit` calls each with a 10-second `timeout()` wrapper. Under the old blocking `log_event()` pattern with a contended write pool, at least some of these would have timed out or deadlocked, failing the 10s bound. The test also verifies the entry count reaches 10, confirming no silent failures. The pattern mirrors the diagnostic scenario described in the investigator report.

### Knowledge Stewardship — Phase 1 Investigator Report

**Status**: PASS

**Evidence**: Addendum comment `IC_kwDORTRSjM7zfnx9` ("Phase 1 Investigator — Knowledge Stewardship Addendum") was posted on GH #308 and contains:

```
## Knowledge Stewardship

- Queried: context_search for MCP connection drops, pool timeout, write pool contention — found entry #2266 confirming the nxs-011 pattern had been previously observed
- Stored: Entry #2299 — "Partial audit-write migration leaves block_in_place contention on write_pool when only some call sites are converted" (lesson-learned, tagged caused_by_feature:nxs-011)
- Declined: N/A
```

`Queried:`, `Stored:`, and `Declined:` entries are all present. Entry #2299 is explicitly attributed to this agent's action. Obligation satisfied.

### Knowledge Stewardship — Phase 2 Rust-Dev Report

**Status**: PASS

**Evidence**: Addendum comment `IC_kwDORTRSjM7zfn4T` ("Phase 2 Rust-Dev — Knowledge Stewardship Addendum") was posted on GH #308 and contains:

```
## Knowledge Stewardship

- Queried: /uni-query-patterns for unimatrix-server write pool and audit patterns — confirmed root cause via entry #2126 (block_in_place starvation pattern)
- Stored: Declined — the fire-and-forget audit write fix is an extension of the already-known GH #302 pattern, not a new reusable discovery
- Declined: New test helper pattern (yield_now after fire-and-forget) not stored — captured by tester in entry #2326
```

`Queried:` entry confirms `/uni-query-patterns` was executed. `Stored:` entry documents the decline with a reason. `Declined:` entry documents the test helper pattern disposition. Obligation satisfied.

## Rework Required

None.

## Notes

The code fix was correct and complete in v1. The only failures in v1 were missing stewardship documentation blocks in the two agent reports. Both addendum comments have been posted to GH #308 and satisfy the stewardship requirements. Gate is now PASS.

## Knowledge Stewardship

- Queried: Unimatrix entries #2266 ("write_pool max_connections=1 + drain task + synchronous block_in_place audit write = pool starvation") and #2299 ("Partial audit-write migration leaves block_in_place contention on write_pool when only some call sites are converted") via context_search — both confirmed relevant.
- Stored: nothing novel to store — the recurring pattern of missing stewardship blocks being resolved via addendum comments is one-off administrative closure, not a systemic lesson warranting a new entry.
