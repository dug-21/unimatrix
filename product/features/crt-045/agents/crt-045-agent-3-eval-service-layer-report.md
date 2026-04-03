# Agent Report: crt-045-agent-3-eval-service-layer

## Task

Implement EvalServiceLayer changes in `crates/unimatrix-server/src/eval/profile/layer.rs`:
1. Add `TypedGraphState::rebuild(&store_arc).await` call (Step 5b) before `with_rate_config()`
2. Post-construction write-back into shared handle after `with_rate_config()` (Step 13b)
3. Add `pub(crate) fn typed_graph_handle()` accessor delegating to `self.inner`
4. Add integration tests per test plan

## Files Modified

- `crates/unimatrix-server/src/eval/profile/layer.rs`
- `crates/unimatrix-server/src/eval/profile/layer_tests.rs`

## Implementation Summary

### layer.rs changes

**Import addition:** Added `TypedGraphState, TypedGraphStateHandle` to the existing
`crate::services` import. These are already `pub use`d from `services/mod.rs` (line 58),
so no new dependency path needed.

**Step 5b (rebuild call):** Added immediately after `store_arc` is created. Uses a
single `match` arm with error inspection via `e.to_string().contains("cycle")` to
distinguish cycle from I/O errors for log messaging. Both error arms set
`rebuilt_state = None` and continue — never abort. Logs `tracing::info!` on success
with profile name and entry count.

**Step 13b (write-back):** Inserted between `with_rate_config()` return and the
`Ok(EvalServiceLayer { ... })` struct construction. Uses
`handle.write().unwrap_or_else(|e| e.into_inner())` for poison recovery.
Drops guard immediately after swap. Logs `tracing::info!("eval TypedGraphState rebuilt")`.

**Accessor:** Added after `has_nli_handle()`, declared `pub(crate)`, no `#[cfg(test)]`
guard (C-04, C-10, ADR-004). Delegates directly to `self.inner.typed_graph_handle()`.

### layer_tests.rs tests added

**`test_from_profile_typed_graph_rebuilt_after_construction`** — Three-layer assertion:
- Layer 1: `handle.read()` confirms `use_fallback=false` and `all_entries.len() >= 2`
- Layer 2: `find_terminal_active(id_a, ...)` returns `Some(id_a)` (graph connectivity)
- Layer 3: `layer.inner.search.search(params, &audit_ctx, &caller_id).await`
  accepts `Ok(_)` or `EmbeddingFailed` (model not loaded in CI); rejects all other errors

**`test_from_profile_returns_ok_on_cycle_error`** — Degraded mode:
- Creates cycle via `UPDATE entries SET supersedes = ...` (raw SQL on the `entries` table)
- **Key discovery:** GRAPH_EDGES rows with `relation_type='Supersedes'` are skipped in
  `build_typed_relation_graph` Pass 2b — cycle detection only fires on edges derived from
  `entries.supersedes` in Pass 2a. Using raw GRAPH_EDGES insertion was the wrong approach.
- Asserts `from_profile()` returns `Ok(layer)` and `guard.use_fallback == true`

## Test Results

- 11/11 `layer_tests` pass
- Full workspace: 0 failures across all test result lines
- `cargo build --workspace`: clean (zero errors in modified files)
- Pre-existing clippy warnings in other crates (unimatrix-engine) — none in modified files

## Issues Encountered

**Supersedes cycle injection (diagnostic):** First attempt used GRAPH_EDGES raw SQL
`INSERT ... relation_type='Supersedes'`. This produced `use_fallback=false` (no cycle).
Root cause: `build_typed_relation_graph` Pass 2b explicitly skips Supersedes rows from
GRAPH_EDGES because they are "already derived from entries.supersedes". The cycle must be
injected via `UPDATE entries SET supersedes = ...` to be visible to Pass 2a and the
subsequent Pass 3 cycle detector.

**Live search EmbeddingFailed in CI:** The embedding model is not available in CI. The
Layer 3 assertion was updated to accept `EmbeddingFailed` as a valid CI outcome per the
test plan comment ("embedding model unavailable in CI is acceptable").

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 14 entries; ADR-001 through ADR-005
  for crt-045 confirmed, patterns #4096 and #4103 surfaced. No new conventions needed from
  briefing.
- Stored: entry #4104 "To trigger Supersedes cycle detection in tests, UPDATE entries.supersedes — not INSERT INTO graph_edges" via /uni-store-pattern
