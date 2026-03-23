# Agent Report: crt-027-agent-5-listener-dispatch

**Agent ID**: crt-027-agent-5-listener-dispatch
**Feature**: crt-027 (WA-4 Proactive Knowledge Delivery)
**Component**: UDS Listener Dispatch Migration
**File**: `crates/unimatrix-server/src/uds/listener.rs`

## Changes Made

### Files Modified
- `/workspaces/unimatrix/crates/unimatrix-server/src/uds/listener.rs`

### Summary of Changes

1. **`dispatch_request` source field wiring**: `HookRequest::ContextSearch` now extracts `source` and passes `source.as_deref().unwrap_or("UserPromptSubmit")` as the `hook` column in the observation row. Previously hardcoded to `"UserPromptSubmit"`.

2. **`handle_compact_payload` migration**: Removed all `BriefingService::assemble()` usage. Now uses `IndexBriefingParams` + `derive_briefing_query()` + `services.briefing.index()`. No `SessionRegistry` lookup for query derivation — UDS path holds state directly.

3. **`format_compaction_payload` rewrite**: New signature `(entries: &[IndexEntry], role, feature, compaction_count, max_bytes, category_histogram) -> Option<String>`. Emits flat indexed table via `format_index_table()` with row-count binary reduction budget enforcement, plus histogram block. No section headers.

4. **`CompactionCategories` struct deleted**: Along with `format_category_section()` and dead budget constants (`DECISION_BUDGET_BYTES`, `INJECTION_BUDGET_BYTES`, `CONVENTION_BUDGET_BYTES`, `CONTEXT_BUDGET_BYTES`).

5. **`HookRequest::Briefing` arm updated**: Migrated from `BriefingService` to `IndexBriefingService` with graceful degradation (empty entries on error, not early return).

6. **11 named tests implemented** per test plan, replacing old `CompactionCategories`-based tests. `make_index_entry()` helper replaces old `make_entry()`.

## Test Results

- `cargo test -p unimatrix-server --lib -- uds::listener`: **106 passed, 0 failed**
- `cargo test --workspace`: **all suites passed, 0 failures**
- `cargo build --workspace`: **0 errors**
- `cargo fmt`: clean
- `cargo clippy -p unimatrix-server`: 0 errors

## Issues Encountered and Resolved

### 1. Private module import path
`use crate::mcp::response::briefing::{IndexEntry, format_index_table}` — `briefing` is a private module. Fix: use the public re-export path `use crate::mcp::response::{IndexEntry, format_index_table}`.

### 2. `assemble()` not found on `IndexBriefingService`
The `HookRequest::Briefing` arm still called `services.briefing.assemble()` after Wave 3 migrated `services.briefing` to `IndexBriefingService`. Fixed by rebuilding that arm with `IndexBriefingParams` and `index()`.

### 3. `dispatch_compact_payload_increments_compaction_count` test failure
Root cause: `handle_compact_payload` early-returned from the `Err` branch before calling `session_registry.increment_compaction()`. In unit tests the embed model is not ready so `index()` always errors, the early return fired, and compaction count stayed at 0. Fix: changed `Err(e) => { warn!(...); return HookResponse::... }` to `Err(e) => { warn!(...); vec![] }` so execution falls through to the increment call regardless of service outcome.

### 4. Edit tool linter conflict (context only)
Concurrent linter modifications caused several Edit tool failures during the session. Resolved via Python atomic string replacement scripts.

## Commit

`72d2fdb` — `impl(listener-dispatch): migrate to IndexBriefingService and source field (#349)`

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — no matching pattern for graceful degradation early-return trap
- Stored: entry #3301 "Graceful degradation via empty fallback, not early return, when post-error side effects must run" via `/uni-store-pattern`

The pattern is non-obvious: the code compiles correctly and works in production (embed model ready), but the early return silently breaks session state increment in unit test environments where the embed service is unavailable. Future agents implementing graceful degradation in UDS handlers should use the empty fallback approach.
