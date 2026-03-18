# Agent Report: dsn-001-agent-8-agent-registry

**Component**: AgentRegistry externalization (`infra/registry.rs` + `store/src/registry.rs`)
**Feature**: dsn-001 (Config Externalization, W0-3)
**Date**: 2026-03-18

---

## Work Completed

### Files Modified

- `crates/unimatrix-store/src/registry.rs`
- `crates/unimatrix-server/src/infra/registry.rs`
- `crates/unimatrix-server/src/main.rs`
- `crates/unimatrix-server/src/server.rs`
- `crates/unimatrix-server/src/uds/mcp_listener.rs`

(Additionally: `src/infra/shutdown.rs` and `src/mcp/identity.rs` were updated by other agents or linter before this agent ran; verified their `AgentRegistry::new` call sites are correct.)

### Changes Implemented

1. **`unimatrix-store/src/registry.rs`**: Added `session_caps: Option<&[Capability]>` as third parameter to `agent_resolve_or_enroll`. `Some(caps)` uses provided capability set; `None` preserves existing permissive/strict branch unchanged. Added 4 tests covering all four paths (IR-02, R-14, AC-06).

2. **`unimatrix-server/src/infra/registry.rs`**: Removed `const PERMISSIVE_AUTO_ENROLL: bool = true`. Added `permissive: bool` and `session_caps: Vec<Capability>` to `AgentRegistry` struct. Updated `AgentRegistry::new` signature to `new(store, permissive: bool, session_caps: Vec<Capability>)`. `resolve_or_enroll` now passes `Some(&self.session_caps)` when non-empty, `None` when empty. Added 4 new tests. Updated all 16 existing test call sites to pass `(store, true, vec![])`.

3. **Call site updates**: All `AgentRegistry::new(store)` occurrences in `main.rs` (2 sites), `server.rs` (1 site), `uds/mcp_listener.rs` (1 site) updated to `AgentRegistry::new(store, true, vec![])` with comment noting dsn-001 startup-wiring agent will supply config values.

---

## Test Results

| Crate | Tests | Pass | Fail |
|-------|-------|------|------|
| `unimatrix-store` (all) | 102 | 102 | 0 |
| New store registry tests | 4 | 4 | 0 |
| `unimatrix-server` registry | N/A | — | — |

Server crate cannot compile for testing due to pre-existing errors in `services/mod.rs` (search-service agent's `ServiceLayer::with_rate_config` missing `boosted_categories` argument) — these existed before this agent ran and are outside this component's scope.

---

## Build Status

- `cargo build -p unimatrix-store`: PASS (zero errors)
- `cargo build -p unimatrix-server`: FAIL (1 pre-existing error in `services/mod.rs` from search-service agent; not caused by this component)

---

## Issues / Blockers

**Pre-existing build error in `unimatrix-server`**: `ServiceLayer::with_rate_config` in `services/mod.rs` missing `boosted_categories: HashSet<String>` argument (from search-service agent's work). This blocks server crate compilation and prevents running server-side registry tests. Not caused by this agent's changes — confirmed by stash test.

No blockers for this component's deliverables.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-store` — no results for capability injection patterns. The parameter-addition pattern follows ADR-002 (no Arc across crate boundaries).
- Stored: nothing novel to store — the `session_caps: Option<&[Capability]>` parameter addition is a straightforward API extension with no runtime gotchas. All behavior is deterministic and the `match session_caps` pattern is idiomatic.
