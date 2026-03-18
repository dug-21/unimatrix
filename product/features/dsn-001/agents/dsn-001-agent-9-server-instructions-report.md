# Agent Report: dsn-001-agent-9-server-instructions

**Feature**: dsn-001 — Config Externalization (W0-3)
**Component**: UnimatrixServer instructions (`server.rs`)
**Date**: 2026-03-18

## Work Completed

### Changes Made

**`crates/unimatrix-server/src/server.rs`** (primary task):
- Renamed `SERVER_INSTRUCTIONS` constant to `SERVER_INSTRUCTIONS_DEFAULT` with updated doc comments explaining it is the private backing fallback value
- Updated `UnimatrixServer::new` doc comment to reference `SERVER_INSTRUCTIONS_DEFAULT` correctly
- Updated `make_server()` test helper to pass `None` as the `instructions` parameter (already had `instructions: Option<String>` param from another agent)
- Updated `AgentRegistry::new` call in `make_server()` from 1-arg to 3-arg form (unblocked by agent-registry work)
- Added `test_server_instructions_none_uses_compiled_default` (AC-01)
- Added `test_server_instructions_some_uses_config_string` (AC-05)

**`crates/unimatrix-server/src/main.rs`**:
- Added `None` as the `instructions` argument to both `UnimatrixServer::new` call sites in `tokio_main_daemon` and `tokio_main_stdio`

**`crates/unimatrix-server/src/uds/mcp_listener.rs`**:
- Added `None` as the `instructions` argument to `UnimatrixServer::new` in the test helper

**`crates/unimatrix-server/src/services/briefing.rs`**:
- Added `boosted_categories` argument to `SearchService::new` call in test helper (unblocked build)

Note: By the time this agent ran, several other agents had already made coordinated changes to `server.rs` (adding the `instructions: Option<String>` parameter and usage in `ServerInfo` construction). My work completed the remaining items: the constant rename, call site updates, and test additions. These changes were committed as part of commit `3fdd9db` by the category-allowlist agent which was writing at the same time.

## Doc Comment Verification

Checked `grep "context_retrospective" crates/unimatrix-server/src/server.rs` — zero results. Doc comments at lines 65, 147, and 207 already updated to `context_cycle_review` by the tool-rename agent.

## Tests

- `test_server_instructions_none_uses_compiled_default` — PASS (AC-01)
- `test_server_instructions_some_uses_config_string` — PASS (AC-05)
- `test_get_info_instructions` — PASS (existing, unchanged assertions)
- Total server tests: 73 passed, 0 failed

## Build Status

- `cargo build --workspace` — PASS (zero errors, 6 warnings pre-existing)
- `cargo test -p unimatrix-server --lib "server::tests::test_server_instructions"` — 2/2 PASS

## Issues / Blockers

None. The swarm environment had multiple agents modifying overlapping files simultaneously. The `SearchService::new` and `AgentRegistry::new` signature changes made by other agents required coordinating call site fixes, but all were resolved.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — no results specific to server instructions externalization
- Stored: nothing novel to store — the pattern (rename const → private default, add Option<String> param, use unwrap_or_else fallback) is a standard Rust backward-compat parameter addition. No runtime traps discovered.
