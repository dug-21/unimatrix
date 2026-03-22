# Security Review: bugfix-311-security-reviewer

## Risk Level: low

## Summary

The fix threads `Arc<ConfidenceParams>` from a single startup resolution site through the full
serving path: `ConfidenceService`, `UsageService`, `StatusService`, `background_tick_loop`, and
`write_lesson_learned`. All production call sites that previously used `ConfidenceParams::default()`
inline now use the operator-configured params. No injection, access-control, deserialization, or
secrets concerns were found.

One observation was flagged and investigated: `UnimatrixServer::record_usage_for_entries()` in
`server.rs` retains a `ConfidenceParams::default()` inline call. After verifying that this method
is only called from within `#[cfg(test)]` blocks (confirmed: zero callers outside `server.rs`; MCP
tools use `self.services.usage.record_access()` exclusively), this is a non-blocking observation
rather than a bug. It does represent a latent technical debt item.

## Findings

### Finding 1 — Observation: `server.rs::record_usage_for_entries` still uses `ConfidenceParams::default()`
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/server.rs:671-676`
- **Description**: The production method `UnimatrixServer::record_usage_for_entries()` (lines
  575-724) calls `ConfidenceParams::default()` inline. However, every call site for this method is
  inside `#[cfg(test)]` (verified: `grep -rn '\.record_usage_for_entries(' src/` returns 20 hits,
  all in `server.rs`; MCP tools exclusively use `self.services.usage.record_access()`). The
  function is never reachable in the production binary. The original bug (GH #311) is fully fixed
  for all live serving paths.
- **Recommendation**: In a follow-up, remove or update `record_usage_for_entries` to accept
  `confidence_params` (or delete it now that `UsageService` fully supersedes it). The method
  comment already notes it is superseded by `UsageService::record_mcp_usage()`.
- **Blocking**: no

### Finding 2 — Eval path uses `ConfidenceParams::default()` by design
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/eval/profile/layer.rs:321`
- **Description**: The eval service layer constructs `Arc::new(ConfidenceParams::default())` for
  eval profiles. The inline comment documents this as intentional ("no operator config in eval
  context"). Eval profiles run against snapshot DBs with fixed baseline behavior; using default
  params is the correct and deterministic choice for reproducible test scenarios.
- **Recommendation**: None required. Correct by design.
- **Blocking**: no

### Finding 3 — Fallback `unwrap_or_else` on `resolve_confidence_params` at startup
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/main.rs:448-451` and `:831-834`
- **Description**: If `resolve_confidence_params(&config)` returns an error (e.g., `Custom` preset
  with missing weights after config mutation), the server falls back silently to
  `ConfidenceParams::default()` with only a `tracing::warn!`. An operator who configured a
  non-default preset may not notice the silent fallback if tracing is not monitored. This is a
  pre-existing pattern and not introduced by this fix, but it is worth noting given the fix's
  purpose is to propagate operator-configured params.
- **Recommendation**: Consider logging at `tracing::error!` level rather than `warn!` for the
  fallback case, to surface the misconfiguration more visibly. Not blocking.
- **Blocking**: no

### Finding 4 — No new unsafe code, no injection, no secrets
- **Severity**: n/a (pass)
- **Location**: all changed files
- **Description**: The diff contains no `unsafe` blocks, no shell command construction, no SQL
  string interpolation with untrusted input, no new external dependencies, and no hardcoded
  secrets or credentials.
- **Blocking**: no

## Arc Threading Correctness

`ConfidenceParams` is a plain data struct (`f64` fields only, `#[derive(Clone)]`). It contains no
`Arc`, `Mutex`, or cyclic references. The fix clones the `Arc` (not the struct itself) into each
sub-service, so all services share a single allocation. The struct is immutable after construction
— no interior mutability — so there are no data race risks. No deadlock is possible from this
change since `Arc<ConfidenceParams>` holds no locks.

The `Arc::clone` pattern used before each `tokio::spawn` closure (`let params =
Arc::clone(&self.confidence_params)`) is the correct Rust idiom for capturing `Arc` values into
async closures. No `Arc` cycle is introduced.

## Parameter Correctness

All six production sites identified in the gate report use the operator-configured params:

1. `ConfidenceService::recompute()` — `Arc::clone(&self.confidence_params)` before spawn.
2. `UsageService::record_mcp_usage()` — `Arc::clone(&self.confidence_params)` before confidence_fn closure.
3. `UsageService::record_briefing_usage()` — `Arc::clone(&self.confidence_params)` before spawn.
4. `StatusService::run_maintenance()` — `&self.confidence_params` reference in batch loop.
5. `tools::write_lesson_learned()` — `&server.services.confidence.confidence_params`.
6. `background_tick_loop`/`run_single_tick`/`StatusService::new()` — promoted from `_confidence_params` stub.

Both daemon (`tokio_main_daemon`) and stdio (`tokio_main_stdio`) startup paths resolve params via
`resolve_confidence_params(&config)` and pass `Arc::clone(&confidence_params)` to both
`ServiceLayer::new()` and `spawn_background_tick()`.

The `ServiceLayer::new()` and `with_rate_config()` signatures are consistent — both accept
`confidence_params` as the final parameter and thread it identically. Test helpers and the test
server use `ConfidenceParams::default()` explicitly, which is correct (no operator config in test
environments).

## Blast Radius Assessment

**Worst case if the fix has a subtle bug**: a calculation error in `Arc::clone(&self.confidence_params)`
capture scope (e.g., wrong params struct being captured) could cause all confidence recomputation
to silently use wrong weights. The failure mode is data quality degradation (confidence scores
wrong), not data corruption, unauthorized access, or denial of service. Entries would still
be stored and retrieved; only their relative ranking would be affected. Recovery requires a
maintenance tick pass to recompute affected entries. The new regression test
`test_compute_confidence_differs_with_non_default_params` would catch this at the unit test level.

**Deadlock risk**: none. The new fields are `Arc<T>` where `T` has no locks.

**Memory leak risk**: negligible. A single `Arc<ConfidenceParams>` allocation is shared across all
sub-services. The struct is a few hundred bytes. No new allocation per-request is introduced.

## Regression Risk

**Low**. The change is purely additive — adding a field to three structs and threading it through
constructors. Existing behavior is preserved for any operator using the default `Collaborative`
preset (weights are identical to `ConfidenceParams::default()`). For non-default presets, the
behavior is now correct (bug fixed). Tests pass (3339 unit, 148 integration per gate report). No
validation logic was changed (the `validation.rs` diff is `cargo fmt` only).

## PR Comments

- Posted 1 comment on PR #347.
- Blocking findings: no.

## Knowledge Stewardship

- Stored: nothing novel to store — the pattern of verifying that "production method still using
  default config" is actually test-only dead code is a specific investigation step rather than a
  generalizable security anti-pattern. The Arc threading correctness pattern for startup resource
  propagation was already stored by the rust-dev agent (entry #3213).
