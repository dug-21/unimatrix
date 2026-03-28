# Agent Report: 430-agent-1-fix

Feature: col-031 (bugfix context)
Bug: GH #430 — write_auto_outcome_entry() writes session telemetry to ENTRIES instead of OUTCOME_INDEX
Branch: bugfix/430-remove-write-auto-outcome-entry

## Changes Made

### crates/unimatrix-server/src/uds/listener.rs
- Deleted `write_auto_outcome_entry()` function (~55 lines, approx lines 1876–1933)
- Removed its call site in `process_session_close()` (the `if !is_abandoned && injection_count > 0` block)
- Removed `is_abandoned` binding (only used by the deleted call site)
- Removed `agent_role` from the state destructure (only passed to the deleted function)
- Added regression test `test_process_session_close_no_entries_written` — asserts zero ENTRIES rows with `topic LIKE 'session/%'` after a full `SessionClose` dispatch with injection_count > 0

### Doc annotations (stale forward references removed/annotated)
- `product/features/col-010/pseudocode/auto-outcomes.md` §4 — retracted false claim that `store.insert()` auto-populates OUTCOME_INDEX
- `product/research/optimizations/server-refactoring-architecture.md:831` — annotated "service-internal caller path" reference as moot (function deleted)
- `product/features/vnc-006/architecture/ADR-002-auditsource-driven-scan-bypass.md` — annotated context block referencing `write_auto_outcome_entry`
- `product/features/col-017/SCOPE.md` — closed open question #2 (auto-outcome topic) as moot
- `product/features/col-017/architecture/ARCHITECTURE.md` — updated flow diagram line referencing `write_auto_outcome_entry()`

## Test Results

- Tests run: `cargo test -p unimatrix-server --lib`
- Pass: 2267
- Fail: 0
- Pre-existing failure `col018_topic_signal_null_for_generic_prompt` (embedding model not ready in test env) was present before this change and is unrelated

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned 15 entries; none directly relevant to the OUTCOME_INDEX write path gap
- Stored: entry #3709 "store.insert() does NOT auto-populate OUTCOME_INDEX — route outcome writes through the MCP path" via /uni-store-pattern
