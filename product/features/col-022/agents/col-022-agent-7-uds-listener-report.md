# Agent Report: col-022-agent-7-uds-listener

## Component
uds-listener (C3): Extend RecordEvent dispatch for cycle_start/cycle_stop events

## Files Modified
- `crates/unimatrix-server/src/infra/session.rs` -- Added `SetFeatureResult` enum and `set_feature_force()` method on `SessionRegistry`
- `crates/unimatrix-server/src/uds/listener.rs` -- Added `handle_cycle_start()`, `update_session_keywords()`, modified `RecordEvent` dispatch to call `handle_cycle_start` for cycle_start events, added 20 tests

## Changes Summary

### session.rs
- `SetFeatureResult` enum: `Set`, `AlreadyMatches`, `Overridden { previous: String }` (col-022, ADR-002)
- `SessionRegistry::set_feature_force()`: Unconditionally sets feature_cycle, overwriting any existing value. Used exclusively by cycle_start events. Returns `SetFeatureResult` indicating what happened. Poison recovery via `unwrap_or_else`.
- 7 unit tests for set_feature_force covering: set-when-absent, already-matches, override-existing, unregistered-session, sequential-different-topics, preserves-heuristic-path

### listener.rs
- Added imports: `SetFeatureResult`, `CYCLE_START_EVENT`, `CYCLE_STOP_EVENT`
- Modified `RecordEvent` handler: Added `if event.event_type == CYCLE_START_EVENT` check before the generic #198 path. After `set_feature_force`, the subsequent `set_feature_if_absent` becomes a no-op.
- `handle_cycle_start()`: Extracts `feature_cycle` from payload, calls `set_feature_force`, persists via fire-and-forget `spawn_blocking`, extracts and persists keywords via separate fire-and-forget task.
- `update_session_keywords()`: Thin wrapper around `store.update_session()` that sets `record.keywords`.
- `cycle_stop`: No special handling -- falls through to generic observation persistence. Feature attribution unchanged.
- 13 integration/unit tests covering: dispatch-sets-feature-force, overwrites-heuristic, already-matches, unknown-session, persists-keywords, no-keywords-field, empty-keywords-stored, cycle-stop-does-not-modify-feature, cycle-stop-without-prior-start, missing-feature-cycle, cycle-start-then-heuristic-is-noop, update-session-keywords-valid/unknown/malformed, constant-agreement

## Test Results
- `set_feature_force` tests: 6 passed
- `cycle_start/stop` dispatch tests: 11 passed
- `update_session_keywords` tests: 3 passed
- **Total new tests: 20 passed, 0 failed**
- Full workspace lib tests: 1171 passed, 0 failed
- Pre-existing integration test failures: 6 in `import_integration.rs` (schema v11->v12 assertion mismatch, owned by schema-migration agent)

## Issues/Blockers
- Pre-existing `import_integration.rs` test failures (6 tests) are from the schema-migration agent's v11->v12 change. These tests hardcode or dynamically read schema version and the import pipeline doesn't properly handle the version bump. Not in this component's scope.
- Pre-existing clippy warnings in `unimatrix-observe` prevent full clippy pass on `unimatrix-server`. My code introduces no new clippy warnings.
- Pre-existing formatting issues in `unimatrix-adapt` and `unimatrix-embed`. My files pass `cargo fmt`.

## Design Decisions
- Keywords are extracted from `event.payload.get("keywords")` and serialized via `.to_string()` (serde_json serialization). This handles arrays like `["a","b"]` correctly since the hook handler serializes them.
- `handle_cycle_start` runs BEFORE the generic #198 path. After `set_feature_force`, `set_feature_if_absent` returns false (feature already set), so no double-write.
- `update_session_keywords` and `update_session_feature_cycle` run as independent fire-and-forget tasks. SQLite serializes them via `BEGIN IMMEDIATE`.
- `set_feature_force` on unregistered sessions returns `SetFeatureResult::Set` (graceful no-op). The observation is still persisted by the caller.

## Knowledge Stewardship
- Queried: /query-patterns for unimatrix-server -- server unavailable, proceeded without
- Stored: nothing novel to store -- implementation followed established patterns (fire-and-forget spawn_blocking, sanitize_metadata_field, update_session read-modify-write). No new gotchas discovered.
