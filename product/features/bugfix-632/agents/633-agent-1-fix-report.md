# Agent Report: 633-agent-1-fix

## Task
Fix `record_provenance()` in `import/mod.rs` to use `log_audit_event()` instead of manual `MAX(event_id)+1` SQL, preventing permanent UNIQUE constraint collisions on `audit_log.event_id` after export/import round-trip (GH#633).

## Changes

### `crates/unimatrix-server/src/import/mod.rs`
- Rewrote `record_provenance()` to accept `&SqlxStore` instead of `&SqlitePool`
- Replaced manual `SELECT COALESCE(MAX(event_id), 0) + 1` + 8-column INSERT with `store.log_audit_event(AuditEvent{...})`
- Fixed latent outcome value: was `1i64` (Denied), now `Outcome::Success`
- Fixed latent column gap: was 8 columns, now all 12 via `log_audit_event`
- Removed unused `std::time::{SystemTime, UNIX_EPOCH}` import
- Added `AuditEvent`, `Outcome` imports from `unimatrix_store`
- Added 2 regression tests with helper `make_audit_log_line()`

### `crates/unimatrix-server/tests/import_integration.rs`
- Fixed `populate_representative_data()` to set `next_audit_id=3` after inserting 3 audit_log rows directly (was leaving counter at 0, which caused the same collision pattern during integration tests)
- Updated `test_round_trip_export_import_reexport` normalization to zero out `next_audit_id` counter values (provenance increments it by 1)
- Updated `test_counter_values_match_export` to expect `next_audit_id` = exported_value + 1

## New Tests
- `test_gh633_log_audit_event_succeeds_after_import` — imports audit rows + counter, calls `log_audit_event()` twice, asserts monotonic IDs > all imported + provenance rows
- `test_gh633_provenance_uses_counter_not_max_plus_one` — imports with counter=5 but max(event_id)=2, verifies provenance at event_id=6 (counter path), not event_id=3 (MAX+1 path)

## Test Results
- Workspace: 5225 passed, 0 failed
- Import unit tests: 22 passed (2 new)
- Import integration tests: 16 passed (0 new, 4 previously failing now fixed)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- found entry #4604 (exact match, describes root cause and prevention), #4396 (TOCTOU race lesson), #4405 (counter name regression). All confirmed fix approach.
- Stored: nothing novel to store -- root cause pattern (counter bypass causing permanent desync) is already documented in entry #4604. The fix approach (delegate to log_audit_event) is the standard pattern already captured in Unimatrix.
