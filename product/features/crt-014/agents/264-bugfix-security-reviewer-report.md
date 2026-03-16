# Security Review: 264-bugfix-security-reviewer

**Agent ID**: 264-bugfix-security-reviewer
**PR**: #265
**Branch**: worktree-bugfix/crt-014-graph-cache
**Feature**: crt-014 / GH #264
**Date**: 2026-03-14

## Risk Level: LOW

## Summary

The fix introduces `SupersessionState` — an `Arc<RwLock<SupersessionState>>` cache that eliminates 4x `Store::query_by_status()` calls from the search hot path, mirroring the pre-existing `EffectivenessStateHandle` pattern. No new external inputs, no new attack surface, no new dependencies. Lock ordering is correct. Cold-start and failure paths degrade conservatively. No blocking findings.

---

## Findings

### Finding 1: `use_fallback` logic on cold-start is correct but requires careful reading

- **Severity**: low (informational)
- **Location**: `services/search.rs` lines 286-300 (branch)
- **Description**: On cold-start, `SupersessionState::new()` sets `use_fallback: true`. When `build_supersession_graph(&[])` is called on the empty snapshot, it returns `Ok(empty_graph)`. The match arm `Ok(graph) => (Some(graph), cached_use_fallback)` preserves `cached_use_fallback = true`, so `use_fallback` remains `true` throughout. The empty `graph_opt` (`Some` but zero-node graph) is then never actually consulted because `use_fallback` gates all downstream lookups. This is correct, but the comment "use_fallback remains true" is the only thing preventing a future reader from assuming the `Ok(graph)` arm means the graph is usable. The invariant that `use_fallback=false` only after a successful `rebuild()` is not directly encoded in the type system.
- **Recommendation**: No code change needed. Correctness is verified by the existing cold-start test. Consider a doc comment on the `Ok(graph)` match arm noting that `cached_use_fallback` from the handle governs whether the graph is used.
- **Blocking**: no

### Finding 2: Supersession state write visibility after `rebuild()` failure

- **Severity**: low (informational)
- **Location**: `background.rs` lines 341-365 (branch), `services/supersession.rs` `rebuild()`
- **Description**: On store error, `rebuild()` returns `Err` and the write guard is never acquired — the old `SupersessionState` is retained. This is the correct safe-failure mode. However, the old state may be arbitrarily stale (e.g., if repeated store errors span multiple ticks, the cache could reflect entries that were subsequently deprecated or deleted). Search results would use stale penalty data but would not crash or produce incorrect access control decisions. The blast radius is limited to stale graph penalties for at most one tick interval (15 min).
- **Recommendation**: Acceptable. The stale-state-on-error behaviour matches how `EffectivenessState` is handled throughout. No action required.
- **Blocking**: no

### Finding 3: `pub use` visibility of `SupersessionState` and `SupersessionStateHandle`

- **Severity**: low (informational)
- **Location**: `services/mod.rs` line 41
- **Description**: `SupersessionState` and `SupersessionStateHandle` are exported `pub` (not `pub(crate)`) from `unimatrix-server`. The fix comment explains this is required because `spawn_background_tick` is `pub` and its parameter types must match. This is consistent with `ConfidenceState`/`ConfidenceStateHandle` and `EffectivenessState`/`EffectivenessStateHandle`. The crate is a binary crate (`[[bin]]`) with a lib target only for tests, so `pub` visibility does not expose these types to external crate consumers in production. No security concern.
- **Recommendation**: No change needed. Pattern is consistent.
- **Blocking**: no

### Finding 4: `all_entries` clone on every search call (memory concern, not a security issue)

- **Severity**: low (informational, not a security finding)
- **Location**: `services/search.rs` line 283 (branch)
- **Description**: The read block clones `guard.all_entries` (a `Vec<EntryRecord>`) on every search call. For a large knowledge base (e.g., 10,000 entries), this clone occurs on every search. This is not a security issue and is explicitly noted in the Option A design comment. If the knowledge base grows substantially, a future optimisation (`Arc<Vec<EntryRecord>>` to share the snapshot) would reduce per-search allocations. Not in scope for this bugfix.
- **Recommendation**: Out of scope. Track as a future profiling item.
- **Blocking**: no

---

## OWASP Assessment

| Concern | Verdict |
|---------|---------|
| Injection (SQL, command, path traversal) | NOT APPLICABLE — no new inputs from external sources; `query_by_status` takes an enum, not user input |
| Broken access control | NOT APPLICABLE — `SupersessionState` contains read-only entry metadata; no privilege escalation possible |
| Security misconfiguration | NOT APPLICABLE — no new configuration surface introduced |
| Vulnerable components | NOT APPLICABLE — no new dependencies added (Cargo.toml unchanged) |
| Data integrity failures | LOW — stale cache on store error is possible; safe failure mode (FALLBACK_PENALTY applied) |
| Deserialization risks | NOT APPLICABLE — no new deserialization paths |
| Input validation gaps | NOT APPLICABLE — no new inputs; `rebuild()` operates only on validated store records |

---

## Concurrency Safety Assessment

**Lock type**: `Arc<RwLock<SupersessionState>>`

**Writer**: background tick only (`run_single_tick`, ~once per 15 min). Acquires write lock after `spawn_blocking` returns. Write lock held for one field assignment (`*guard = new_state`). No other lock held simultaneously.

**Reader**: `SearchService::search()`. Acquires read lock, clones two fields, drops guard. Read lock is released before any other lock is acquired (verified: no subsequent lock acquisitions use the guard; the read block is a closed brace expression with no await points).

**Lock ordering (R-01)**: Compliant. The read lock is fully released (guard dropped by end of the `{ ... }` block) before `build_supersession_graph` executes. There is no code path where the supersession read lock is held concurrently with any other lock.

**Deadlock risk**: None detected. Single-writer / multiple-reader RwLock with no nested lock acquisition. Matches the `EffectivenessStateHandle` pattern exactly.

**Poison recovery**: All lock acquisitions use `.unwrap_or_else(|e| e.into_inner())` in both the writer and the reader paths. A panic in the writer tick will leave the RwLock poisoned but readable — the old (pre-panic) state is returned by `into_inner()`. This is the established project convention (CategoryAllowlist, EffectivenessState).

**Data race risk**: None. `Arc<RwLock<_>>` provides the required mutual exclusion. No `unsafe` code introduced.

---

## Blast Radius Assessment

**Worst case if `SupersessionState::rebuild()` fails or panics:**
- `rebuild()` returns `Err(StoreError)` — tick logs an error and retains the old state. Search continues with the previous snapshot. No crash.
- `rebuild()` panics inside `spawn_blocking` — `tokio::task::spawn_blocking` catches the panic as a `JoinError`. The `Err(JoinError)` arm logs the panic and retains the old state. No crash.
- Old state is perpetually stale: search applies `FALLBACK_PENALTY` (0.70) to all superseded/deprecated entries. This is the same behaviour as pre-crt-014. Knowledge graph topology is not consulted but search availability is preserved.

**Worst case for correctness regression:**
- A deprecation or supersession event occurs AFTER the last background tick. For up to 15 minutes, the cached snapshot does not reflect the new edge. The graph built from the stale snapshot will not penalise the newly-deprecated entry with topology-derived penalties. Instead it will either use the old penalty (if the entry was already in the snapshot with its old status) or no penalty (if the entry was just deprecated and not yet in the cache). This is a bounded correctness window, not a safety issue, and is a known trade-off of the caching approach.

**Worst case for availability:**
- A write lock is held briefly (~microseconds, field assignment only). No search call can be starved. RwLock allows multiple concurrent readers; the single 15-minute writer does not block reads for any meaningful duration.

---

## Regression Risk Assessment

**Risk 1: Cold-start search correctness** — COVERED by `test_search_uses_cached_supersession_state_cold_start_fallback`. Cold-start correctly applies `FALLBACK_PENALTY` (same as pre-crt-014).

**Risk 2: Post-rebuild search correctness** — COVERED by `test_search_uses_cached_supersession_state_after_rebuild`. Simulates background tick write + search path read in isolation.

**Risk 3: Graph build on empty slice** — COVERED in `graph.rs` tests (`empty_entry_slice_is_valid_dag`). `build_supersession_graph(&[])` returns `Ok` with a zero-node graph, not `Err`. The cold-start `use_fallback=true` gates all penalty lookups so the empty graph is never consulted.

**Risk 4: Stale window between write and first tick** — Pre-existing risk of the caching design. Conservative fallback means stale data produces over-penalisation (entries that should have topology-derived penalties get the blunt `FALLBACK_PENALTY`), not under-penalisation. Safe direction of failure.

**Risk 5: Test divergence (uncommitted test change)** — Gate report documents that `test_concurrent_search_stability` has an uncommitted change (parallel 10s budget → sequential 30s budget). This is a WARN in the gate report. The committed (parallel) version is more aggressive; the on-disk (sequential) version passed smoke. The committed version should be reconciled before merge. This is not a security issue but is a test integrity concern.

**No regressions identified in the search pipeline logic.** The penalty and injection code paths are identical to pre-fix; only the entry-loading mechanism changed (per-query store I/O → cached handle read).

---

## Secrets and Hardcoded Credentials

None. No API keys, tokens, passwords, or credentials in the diff. No `.env` modifications.

---

## New Dependencies

None. `Cargo.toml` is unchanged. The fix uses only `std::sync::{Arc, RwLock}`, existing workspace crates (`unimatrix_core`, `unimatrix_store`), and existing tokio primitives.

---

## PR Comments

Posted 1 comment on PR #265. No blocking findings — approving.

---

## Knowledge Stewardship

- Stored: nothing novel to store — this is a feature-specific security review. The lock ordering pattern (`acquire read lock, clone fields, drop guard before any other lock`) and poison recovery convention (`unwrap_or_else(|e| e.into_inner())`) are already established project patterns. No new anti-pattern or generalizable lesson identified.
